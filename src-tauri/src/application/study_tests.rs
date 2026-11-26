use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use tempfile::tempdir;

use crate::application::AppState;
use crate::domain::{
    CompareAgentsRequest, CreateReferenceMaterialRequest, DecideSolutionRevisionRequest,
    DocumentFilter, FullTextSearchRequest, GenerateSolutionRequest, ProposeSolutionRevisionRequest,
    RecognizeProblemsRequest, RunScienceToolRequest, SaveAgentProfileRequest,
    SendChatMessageRequest, UpsertProblemRequest,
};

const TINY_PNG_BYTES: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1F, 0x15, 0xC4,
    0x89, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9C, 0x63, 0xF8, 0xCF, 0xC0, 0xF0,
    0x1F, 0x00, 0x05, 0x00, 0x01, 0xFF, 0x89, 0x99, 0x3D, 0x1D, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[tokio::test]
async fn test_phase5_partial_revision_diff_approval_rejection_and_scope_isolation() {
    let dir = tempdir().expect("tempdir");
    let state = AppState::initialize(dir.path().to_path_buf())
        .await
        .expect("init");
    let boot = state.bootstrap().await.expect("bootstrap");
    let math_id = boot.subjects[0].id.clone();

    let profiles = state.list_agent_profiles().await.expect("profiles");
    let mock_profile_id = profiles
        .iter()
        .find(|p| p.provider_kind == "mock")
        .expect("mock profile")
        .id
        .clone();

    let imported = state
        .import_file_bytes(
            &math_id,
            None,
            "gaussian_and_linear.png",
            &STANDARD.encode(TINY_PNG_BYTES),
        )
        .await
        .expect("import");
    let doc_id = imported.detail.document.id.clone();

    // Create two distinct problems in the SAME document to verify strict scope isolation
    let prob1 = state
        .upsert_problem(&UpsertProblemRequest {
            id: None,
            document_id: doc_id.clone(),
            problem_number: "第1問".into(),
            subproblem_number: None,
            subject_category: "mathematics".into(),
            field: Some("calculus".into()),
            statement_markdown: "ガウス積分 $I = \\int_0^\\infty e^{-x^2}\\,dx$ を求めよ。".into(),
            given_conditions: vec![],
            target_objective: Some("I の厳密値".into()),
            source_pages: vec![1],
            figure_refs: vec![],
            uncertainties: vec![],
            is_user_verified: true,
        })
        .await
        .expect("prob1");

    let prob2 = state
        .upsert_problem(&UpsertProblemRequest {
            id: None,
            document_id: doc_id.clone(),
            problem_number: "第2問".into(),
            subproblem_number: None,
            subject_category: "mathematics".into(),
            field: Some("linear_algebra".into()),
            statement_markdown: "行列 $A$ の固有値を求めよ。".into(),
            given_conditions: vec![],
            target_objective: Some("固有値".into()),
            source_pages: vec![1],
            figure_refs: vec![],
            uncertainties: vec![],
            is_user_verified: true,
        })
        .await
        .expect("prob2");

    // Generate v1 solution for BOTH prob1 and prob2
    let sol1_v1 = state
        .generate_solution(&GenerateSolutionRequest {
            document_id: doc_id.clone(),
            problem_id: Some(prob1.id.clone()),
            agent_profile_id: Some(mock_profile_id.clone()),
            model_name: None,
            mode: "detailed".into(),
            target_scope: "single_problem".into(),
            page_range_start: None,
            page_range_end: None,
            repair_feedback: None,
            parent_solution_id: None,
            reference_material_ids: vec![],
            user_attempt_markdown: None,
            user_attempt_images: vec![],
        })
        .await
        .expect("sol1 v1");
    assert_eq!(sol1_v1.solution.version_number, 1);
    assert!(sol1_v1.solution.pdf_attachment_id.is_some());

    let sol2_v1 = state
        .generate_solution(&GenerateSolutionRequest {
            document_id: doc_id.clone(),
            problem_id: Some(prob2.id.clone()),
            agent_profile_id: Some(mock_profile_id.clone()),
            model_name: None,
            mode: "concise".into(),
            target_scope: "single_problem".into(),
            page_range_start: None,
            page_range_end: None,
            repair_feedback: None,
            parent_solution_id: None,
            reference_material_ids: vec![],
            user_attempt_markdown: None,
            user_attempt_images: vec![],
        })
        .await
        .expect("sol2 v1");
    let sol2_original_json = sol2_v1.solution.structured_json.clone();

    let conv = state
        .create_conversation(
            &doc_id,
            Some(&prob1.id),
            Some(&sol1_v1.solution.id),
            "修正相談",
        )
        .await
        .expect("conv");

    // 1. Propose partial revision -> MUST stay pending and NEVER auto-mutate solutions!
    let proposed_reject = state
        .propose_solution_revision(&ProposeSolutionRevisionRequest {
            conversation_id: conv.id.clone(),
            document_id: doc_id.clone(),
            base_solution_id: sol1_v1.solution.id.clone(),
            instruction: "ヤコビアンの絶対値の説明を追記してください".into(),
            agent_profile_id: Some(mock_profile_id.clone()),
            model_name: None,
            reference_material_ids: vec![],
        })
        .await
        .expect("propose revision");
    assert_eq!(
        proposed_reject.assistant_message.revision_status.as_deref(),
        Some("pending")
    );
    let patch = proposed_reject
        .assistant_message
        .revision_patch
        .as_ref()
        .expect("revision patch present");
    assert!(
        patch.hunks.len() >= 2,
        "expected >= 2 structured diff hunks, got {:?}",
        patch.hunks
    );

    // Before approval: still ONLY v1 in DB (no unconditional auto-apply!)
    let sols_before = state
        .list_solutions(&doc_id, Some(&prob1.id))
        .await
        .expect("list");
    assert_eq!(sols_before.len(), 1);

    // 2. Reject proposal -> ZERO new versions created!
    let rejected = state
        .decide_solution_revision(&DecideSolutionRevisionRequest {
            message_id: proposed_reject.assistant_message.id.clone(),
            decision: "reject".into(),
            approved_hunk_ids: None,
        })
        .await
        .expect("reject");
    assert_eq!(
        rejected.message.revision_status.as_deref(),
        Some("rejected")
    );
    assert!(rejected.new_solution.is_none());
    let sols_after_reject = state
        .list_solutions(&doc_id, Some(&prob1.id))
        .await
        .expect("list");
    assert_eq!(sols_after_reject.len(), 1);

    // 3. Propose again and partially approve ONLY the added step hunk (not the answer text hunk)
    let proposed_approve = state
        .propose_solution_revision(&ProposeSolutionRevisionRequest {
            conversation_id: conv.id.clone(),
            document_id: doc_id.clone(),
            base_solution_id: sol1_v1.solution.id.clone(),
            instruction: "極座標ヤコビアンの厳密な説明を追加してください".into(),
            agent_profile_id: Some(mock_profile_id.clone()),
            model_name: None,
            reference_material_ids: vec![],
        })
        .await
        .expect("propose 2");
    let step_hunk_id = proposed_approve
        .assistant_message
        .revision_patch
        .as_ref()
        .unwrap()
        .hunks
        .iter()
        .find(|h| h.hunk_id.starts_with("steps:"))
        .expect("step hunk")
        .hunk_id
        .clone();

    let approved = state
        .decide_solution_revision(&DecideSolutionRevisionRequest {
            message_id: proposed_approve.assistant_message.id.clone(),
            decision: "approve".into(),
            approved_hunk_ids: Some(vec![step_hunk_id]),
        })
        .await
        .expect("approve");
    assert_eq!(
        approved.message.revision_status.as_deref(),
        Some("approved")
    );
    let sol1_v2 = approved.new_solution.expect("new v2 solution created");
    assert_eq!(sol1_v2.solution.version_number, 2);
    assert!(sol1_v2.solution.is_current);
    assert_eq!(
        sol1_v2.solution.parent_solution_id.as_deref(),
        Some(sol1_v1.solution.id.as_str())
    );
    assert!(
        sol1_v2.solution.pdf_attachment_id.is_some(),
        "approved revision must recompile Typst PDF"
    );
    assert!(sol1_v2.latex_errors.is_empty());

    // Double-approval must be blocked cleanly
    assert!(
        state
            .decide_solution_revision(&DecideSolutionRevisionRequest {
                message_id: proposed_approve.assistant_message.id.clone(),
                decision: "approve".into(),
                approved_hunk_ids: None,
            })
            .await
            .is_err()
    );

    // Non-destructive history preserved (v1 untouched, v2 current)
    let prob1_versions = state
        .list_solutions(&doc_id, Some(&prob1.id))
        .await
        .expect("versions");
    assert_eq!(prob1_versions.len(), 2);
    assert!(
        prob1_versions
            .iter()
            .any(|s| s.version_number == 1 && !s.is_current)
    );
    assert!(
        prob1_versions
            .iter()
            .any(|s| s.version_number == 2 && s.is_current)
    );

    // Scope isolation: prob2's solution is 100% untouched!
    let prob2_versions = state
        .list_solutions(&doc_id, Some(&prob2.id))
        .await
        .expect("prob2 versions");
    assert_eq!(prob2_versions.len(), 1);
    assert_eq!(prob2_versions[0].structured_json, sol2_original_json);

    // Version-to-version diff reflects ONLY the approved step addition (not unapproved answer edit)
    let vdiff = state
        .compare_solution_versions(&sol1_v1.solution.id, &sol1_v2.solution.id)
        .await
        .expect("version diff");
    assert_eq!(vdiff.added_count, 1);
    assert_eq!(vdiff.modified_count, 0);
    assert_eq!(vdiff.hunks[0].field_path, "steps[4]");
}

#[tokio::test]
async fn test_phase5_review_modes_references_multi_agent_fts5_and_physics_diagrams() {
    let dir = tempdir().expect("tempdir");
    let state = AppState::initialize(dir.path().to_path_buf())
        .await
        .expect("init");
    let boot = state.bootstrap().await.expect("bootstrap");
    let math_id = boot.subjects[0].id.clone();
    let phys_id = boot.subjects[1].id.clone();

    let profiles = state.list_agent_profiles().await.expect("profiles");
    let mock_a = profiles
        .iter()
        .find(|p| p.provider_kind == "mock")
        .expect("mock")
        .id
        .clone();
    let claude_id = profiles
        .iter()
        .find(|p| p.provider_kind == "claude_code")
        .expect("claude")
        .id
        .clone();

    // Create a second mock profile to test permitted multi-agent side-by-side comparison
    let mock_b = state
        .save_agent_profile(&SaveAgentProfileRequest {
            id: None,
            name: "Mock Agent B (比較検証)".into(),
            provider_kind: "mock".into(),
            executable_path: None,
            args: vec![],
            default_model: Some("mock-fast".into()),
            auth_mode: "official_harness".into(),
            is_enabled: true,
            is_default: false,
        })
        .await
        .expect("mock B")
        .id;

    let imported = state
        .import_file_bytes(
            &math_id,
            None,
            "handwritten_attempt.png",
            &STANDARD.encode(TINY_PNG_BYTES),
        )
        .await
        .expect("import");
    let doc_id = imported.detail.document.id.clone();

    // 1. Study status + favorite management
    let updated = state
        .update_document(
            &doc_id,
            Some("微積分：手書き答案添削"),
            None,
            None,
            Some(true),
            Some("handwritten_answer"),
            Some("review_needed"),
        )
        .await
        .expect("update doc");
    assert_eq!(updated.document.study_status, "review_needed");
    assert!(updated.document.is_favorite);
    assert_eq!(updated.document.recognition_mode, "handwritten_answer");

    let filtered = state
        .list_documents(&DocumentFilter {
            study_status: Some("review_needed".into()),
            favorites_only: true,
            ..Default::default()
        })
        .await
        .expect("filter");
    assert_eq!(filtered.len(), 1);
    assert_eq!(filtered[0].id, doc_id);

    // 2. Handwritten answer recognition mode (Mode 4)
    let rec = state
        .recognize_problems(&RecognizeProblemsRequest {
            document_id: doc_id.clone(),
            agent_profile_id: Some(mock_a.clone()),
            model_name: None,
            recognition_mode: "handwritten_answer".into(),
            page_range_start: None,
            page_range_end: None,
            page_images: vec![],
            cropped_region_png_base64: None,
        })
        .await
        .expect("recognize handwritten");
    assert_eq!(rec.problems.len(), 1);
    assert!(
        rec.problems[0]
            .statement_markdown
            .contains("手書き答案書き起こし")
    );
    assert!(!rec.problems[0].uncertainties.is_empty());

    // 3. Untrusted reference material (lecture note) + review & beginner modes
    let ref_note = state
        .create_reference_material(&CreateReferenceMaterialRequest {
            subject_id: Some(math_id.clone()),
            document_id: Some(doc_id.clone()),
            title: "微積分講義ノート第3回（べき乗の積分）".into(),
            kind: "lecture_note".into(),
            content_markdown: "定理: べき乗関数 $\\int x^n\\,dx = \\frac{x^{n+1}}{n+1} + C$（ルベーグ積分およびリーマン積分の基本公式）".into(),
        })
        .await
        .expect("create ref");
    let listed_refs = state
        .list_reference_materials(Some(&doc_id), None)
        .await
        .expect("list refs");
    assert_eq!(listed_refs.len(), 1);

    let review_sol = state
        .generate_solution(&GenerateSolutionRequest {
            document_id: doc_id.clone(),
            problem_id: Some(rec.problems[0].id.clone()),
            agent_profile_id: Some(mock_a.clone()),
            model_name: None,
            mode: "review".into(),
            target_scope: "single_problem".into(),
            page_range_start: None,
            page_range_end: None,
            repair_feedback: None,
            parent_solution_id: None,
            reference_material_ids: vec![ref_note.id.clone()],
            user_attempt_markdown: Some("$\\int_0^1 x^2\\,dx = [x^2/2]_0^1 = 1/2$".into()),
            user_attempt_images: vec![],
        })
        .await
        .expect("review solution");
    assert_eq!(review_sol.solution.mode, "review");
    assert!(review_sol.solution.pdf_attachment_id.is_some());

    let beginner_sol = state
        .generate_solution(&GenerateSolutionRequest {
            document_id: doc_id.clone(),
            problem_id: Some(rec.problems[0].id.clone()),
            agent_profile_id: Some(mock_a.clone()),
            model_name: None,
            mode: "beginner".into(),
            target_scope: "single_problem".into(),
            page_range_start: None,
            page_range_end: None,
            repair_feedback: None,
            parent_solution_id: None,
            reference_material_ids: vec![ref_note.id.clone()],
            user_attempt_markdown: None,
            user_attempt_images: vec![],
        })
        .await
        .expect("beginner solution");
    assert_eq!(beginner_sol.solution.mode, "beginner");
    assert!(beginner_sol.solution.pdf_attachment_id.is_some());

    // 4. Multi-agent parallel comparison with compliance gate on Claude official_harness
    let cmp = state
        .compare_agents_generate_solutions(&CompareAgentsRequest {
            document_id: doc_id.clone(),
            problem_id: Some(rec.problems[0].id.clone()),
            mode: "concise".into(),
            profile_ids: vec![mock_a.clone(), mock_b.clone(), claude_id.clone()],
            reference_material_ids: vec![ref_note.id.clone()],
            user_attempt_markdown: None,
        })
        .await
        .expect("compare agents");
    assert_eq!(cmp.entries.len(), 3);
    assert!(cmp.entries[0].solution_result.is_some());
    assert!(cmp.entries[1].solution_result.is_some());
    assert!(cmp.entries[2].solution_result.is_none());
    assert!(
        cmp.entries[2]
            .error
            .as_deref()
            .unwrap_or("")
            .contains("利用規約保護")
    );
    assert!(cmp.diff_between_first_two.is_some());

    // 5. Chat message with reference material + SQLite FTS5 cross-entity Japanese search
    let conv = state
        .create_conversation(&doc_id, Some(&rec.problems[0].id), None, "質問チャット")
        .await
        .expect("conv");
    state
        .send_chat_message(&SendChatMessageRequest {
            run_id: None,
            conversation_id: conv.id.clone(),
            document_id: doc_id.clone(),
            agent_profile_id: Some(mock_a.clone()),
            model_name: None,
            content_markdown: "フーリエ変換の収束条件について教えてください".into(),
            referenced_problem_id: Some(rec.problems[0].id.clone()),
            include_problem_context: true,
            include_solution_context: true,
            attached_images: vec![],
            reference_material_ids: vec![ref_note.id.clone()],
        })
        .await
        .expect("chat");

    // FTS5 trigram (>=3 Japanese chars) across message, reference, solution, problem
    let hit_msg = state
        .full_text_search(&FullTextSearchRequest {
            query: "フーリエ変換".into(),
            ..Default::default()
        })
        .await
        .expect("fts msg");
    assert!(hit_msg.iter().any(|h| h.entity_type == "message"));

    let hit_ref = state
        .full_text_search(&FullTextSearchRequest {
            query: "ルベーグ積分".into(),
            ..Default::default()
        })
        .await
        .expect("fts ref");
    assert!(hit_ref.iter().any(|h| h.entity_type == "reference"));

    let hit_sol = state
        .full_text_search(&FullTextSearchRequest {
            query: "ヤコビアン".into(),
            study_status: Some("review_needed".into()),
            favorites_only: true,
            ..Default::default()
        })
        .await
        .expect("fts sol");
    assert!(hit_sol.iter().any(|h| h.entity_type == "solution"));

    // Short 2-char Japanese query fallback
    let hit_short = state
        .full_text_search(&FullTextSearchRequest {
            query: "添削".into(),
            ..Default::default()
        })
        .await
        .expect("fts short");
    assert!(!hit_short.is_empty());

    let rebuilt_count = state.rebuild_search_index().await.expect("rebuild fts");
    assert!(rebuilt_count >= 5);

    // 6. Real local Science MCP Phase 5 advanced physics diagram tool + PDF embed
    let phys_doc = state
        .import_file_bytes(
            &phys_id,
            None,
            "incline_fbd.png",
            &STANDARD.encode(TINY_PNG_BYTES),
        )
        .await
        .expect("phys import")
        .detail
        .document;
    let phys_prob = state
        .upsert_problem(&UpsertProblemRequest {
            id: None,
            document_id: phys_doc.id.clone(),
            problem_number: "問1".into(),
            subproblem_number: None,
            subject_category: "physics".into(),
            field: Some("classical_mechanics".into()),
            statement_markdown: "斜面上の質点の自由物体図とポテンシャルエネルギー曲線を描け。"
                .into(),
            given_conditions: vec![],
            target_objective: None,
            source_pages: vec![1],
            figure_refs: vec![],
            uncertainties: vec![],
            is_user_verified: true,
        })
        .await
        .expect("phys prob");
    let phys_sol = state
        .generate_solution(&GenerateSolutionRequest {
            document_id: phys_doc.id.clone(),
            problem_id: Some(phys_prob.id.clone()),
            agent_profile_id: Some(mock_a.clone()),
            model_name: None,
            mode: "standard".into(),
            target_scope: "single_problem".into(),
            page_range_start: None,
            page_range_end: None,
            repair_feedback: None,
            parent_solution_id: None,
            reference_material_ids: vec![],
            user_attempt_markdown: None,
            user_attempt_images: vec![],
        })
        .await
        .expect("phys sol");

    // Execute real local Python Science MCP `plot_free_body_diagram` and embed into Typst PDF!
    let fbd_res = state
        .run_science_tool(&RunScienceToolRequest {
            document_id: phys_doc.id.clone(),
            solution_id: Some(phys_sol.solution.id.clone()),
            tool_name: "plot_free_body_diagram".into(),
            arguments: serde_json::json!({
                "forces": [
                    { "label": "mg", "fx": 0.0, "fy": -9.8 },
                    { "label": "N", "fx": 0.0, "fy": 9.8 }
                ],
                "body_label": "m",
                "title": "Free-Body Diagram"
            }),
            attach_to_solution_pdf: true,
        })
        .await
        .expect("run plot_free_body_diagram");
    assert_eq!(fbd_res.execution.status, "succeeded");
    assert_eq!(fbd_res.created_artifacts.len(), 1);
    let updated_phys = fbd_res
        .updated_solution
        .expect("FBD diagram embedded into new PDF version");
    assert_eq!(updated_phys.solution.version_number, 2);
    assert!(updated_phys.solution.pdf_attachment_id.is_some());
    assert!(updated_phys.pdf_error.is_none());
}
