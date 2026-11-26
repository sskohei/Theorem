# 実装・技術検証状況（Implementation Status）

> **最終更新**: 2026-11-26
> **現在フェーズ**: **Phase 2（AI 機能統合 `#3`）・アプリ名 `Theorem` 改称 実装・ローカル自動検証・実機 OpenAI Codex E2E 検証・Windows 11 起動スモーク検証完了**（Phase 3 Science MCP Server `#4` 着手可能；未導入の Claude Code 実機接続および Phase 0/4 の実 Google Drive OAuth は引き続き保留として記録）
> **誠実性ルール**: 未実装機能を完成済みと記載しない。モックテスト合格と外部サービス実接続確認を明確に区別する。外部認証が必要な項目は認証情報なしで「動作確認済み」にしない。

---

## 1. ドキュメント整備状況

| ドキュメント | 状態 | 備考 |
|---|---|---|
| `README.md` / `AGENTS.md` | 更新済み | アプリ名 `Theorem` 改称および Phase 2 完了を反映 |
| `docs/product-specification.md` | 作成済み (v1.0) | 製品全体仕様・MVP 条件 |
| `docs/requirements.md` | 作成済み (v1.0) | 機能・非機能要件・禁止事項 |
| `docs/architecture.md` | 作成済み (v1.0) | 全体構成・参考 OSS 調査・ストレージ設計 |
| `docs/database.md` | 作成済み (v1.0) | 全 21 テーブル・ER 図・インデックス・マイグレーション計画 |
| `docs/agent-integration.md` | 作成済み (v1.0) | 共通 IF・イベント・Capability・プロセス管理 |
| `docs/provider-compatibility.md` | 更新済み (Phase 0 反映) | Codex / Antigravity / Custom ACP 検証結果・Claude 制約記録 |
| `docs/science-mcp.md` | 作成済み (v1.0) | ツール仕様・AST 安全パーサー設計 |
| `docs/pdf-generation.md` | 更新済み (v1.1) | Embedded Typst 0.15.1 + `mitex` + 互換プレリュード確定 |
| `docs/google-drive.md` | 作成済み (v1.0) | OAuth PKCE・`drive.file`・永続ジョブキュー |
| `docs/security.md` | 作成済み (v1.0) | 脅威モデル・隔離・Tauri Capability/CSP |
| `docs/ui-design.md` | 作成済み (v1.0) | 3 カラム画面・6 タブ・11 設定カテゴリ |
| `docs/testing.md` | 作成済み (v1.0) | Rust/React/Python/統合テスト計画 |
| `docs/development.md` | 更新済み (v1.2) | Phase 2 検証手順・ライブ Codex テストコマンド・規約反映 |
| `docs/roadmap.md` | 更新済み (v1.1) | Phase 0〜6 計画・GitHub Issue (`#1`〜`#7`) 対応 |
| `docs/adr/` (ADR-0001〜0006) | 更新済み | **全 6 件 Accepted**（ADR-0004 を Phase 0 検証結果で確定） |
| `poc/README.md` | 作成済み | Phase 0 実行可能 PoC の実行手順・実測レポート |
| `schemas/`・`templates/` | 作成済み (Phase 2) | `recognition.v1.schema.json` / `solution.v1.schema.json`・検証済み Typst テンプレート 5 種 |

---

## 2. Phase 0 技術検証（PoC）トラッカー

ステータス凡例: `動作確認済み（実機）` / `動作確認済み（モック）` / `未検証` / `対応不可` / `利用条件の確認が必要`

### 2.1 AI ハーネス PoC（`poc/harness-poc`, `poc/mock-acp-agent`）

| # | 検証項目 | ステータス | 実測結果・代替手段・技術的リスク |
|---|---|---|---|
| 1 | Codex App Server の起動 | `動作確認済み（実機）` | `codex-cli 0.172.0` (`codex app-server --stdio`) で `initialize`/`initialized` 成功。Windows Job Object で孤児プロセス防止。 |
| 2 | Claude Code への接続 | `未検証` | 本検証環境に `claude` CLI 未導入。API キー利用を正規代替手段とし、導入可能環境で追試予定。 |
| 3 | Antigravity ACP への接続 | `動作確認済み（実機）` | `agy 1.5.12` (`agy acp --stdio --sandbox --disable-telemetry`) で `initialize`/`session/new` 成功。 |
| 4 | カスタム ACP エージェントへの接続 | `動作確認済み（モック）` | `poc/mock-acp-agent/acp_echo_agent.py` で汎用 ACP クライアント＋Capability 判定を `cargo test` 検証済み。 |
| 5 | 公式認証の確認 | `動作確認済み（実機）` | Codex: `OPENAI_API_KEY` 除去下で `account/read` → `chatgpt` 確認。Antigravity: 既存認証継承・`authMethods` 4 種取得確認。Claude Code のみ `未検証`。 |
| 6 | サブスクリプション利用条件の確認 | `利用条件の確認が必要` | Claude Code はサードパーティ OAuth 流用禁止制約あり（API キー代替）。Codex / Antigravity も対話利用限定・無人バッチ禁止として継続確認。 |
| 7 | 画像入力 | `動作確認済み（実機）` | 生成した数式 PNG（`2X + 3 = 11`）に対し Codex（`localImage`）・Antigravity（base64 `image`）ともに `x = 4` を正確に回答。 |
| 8 | ストリーミング応答 | `動作確認済み（実機）` | Codex (`item/agentMessage/delta` 24 デルタ)・Antigravity (`session/update`)・Custom ACP すべて逐次受信を確認。 |
| 9 | セッション再開 | `動作確認済み（実機）` | Codex (`thread/resume`)・Antigravity (`session/load`)・Custom ACP (`session/load`) すべて成功。非対応エージェント向け SQLite 引き継ぎ方針を維持。 |
| 10 | 処理のキャンセル | `動作確認済み（実機）` | Codex (`turn/interrupt` → `status=interrupted`) 成功。Antigravity (`session/cancel`) は送信後に残留バッファチャンクが生じ得るリスクを発見（Phase 2 でターン境界ドレイン対策を実装）。 |
| 11 | MCP ツールの登録 | `動作確認済み（実機）` | Codex は `-c mcp_servers.poc_science...` で `~/.codex/config.toml` 非改変（前後 SHA-256 一致）のまま登録確認。Antigravity は `session/new` の `mcpServers` 受理を確認。 |
| 12 | 権限制御 | `動作確認済み（実機）` | Codex (`sandbox="read-only"`, `approvalPolicy="untrusted"`) および Antigravity (`--sandbox`) の双方でシェル実行許可要求をクライアント側で拒否し実行を阻止。 |

### 2.2 PDF / Typst PoC（`poc/pdf-typst-poc`, `poc/pdfjs-poc`）

| # | 検証項目 | ステータス | 実測結果・代替手段・技術的リスク |
|---|---|---|---|
| 1 | Typst による PDF 生成 | `動作確認済み（実機）` | Embedded `typst 0.15.1` + `typst-pdf 0.15.1` で A4 3 ページ PDF を 421 ms（コンパイル 214 ms + 出力 207 ms）で生成。`SandboxedWorld` で `#read(...)` と外部パッケージ取得を遮断（[ADR-0004](./adr/0004-typst-pdf-generation-and-latex-conversion.md) 確定）。 |
| 2 | 日本語の組版 | `動作確認済み（実機）` | `Noto Serif JP`（SIL OFL 1.1）+ `typst-assets` による日本語本文・見出し・数式混在組版を確認。 |
| 3 | 大学数学の複雑な数式 | `動作確認済み（実機）` | 偏微分・多重/周回積分・行列・固有値・場合分け・量子力学ブラケット・Cauchy-Schwarz 証明・複数行ラグランジアン整列を確認。 |
| 4 | LaTeX から Typst への変換 | `動作確認済み（実機）` | `mitex 0.2.4`（生 6/12）vs `tex2typst-rs 0.4.1`（生 9/12）を比較。`mitex` + Typst 0.15 シンボル正規化 + オフライン互換プレリュードで **12/12（100%）合格** かつ未対応マクロ即座検出を確認。 |
| 5 | 図・グラフの埋め込み | `動作確認済み（実機）` | 減衰振動 SVG（ベクター）および 480×240 PNG（ラスター）の双方を PDF へ埋め込み成功。 |
| 6 | PDF.js による PDF 表示 | `動作確認済み（実機）` | `pdfjs-dist 5.7.284` + Vite ビルド・TS strict 通過。生成 PDF 3 ページ（`595.3 x 841.9 pt`）読み込みと日本語・数式テキスト抽出を `verify_pdfjs.mjs` で確認。 |

### 2.3 Google Drive PoC（`poc/gdrive-poc`）

| # | 検証項目 | ステータス | 実測結果・代替手段・技術的リスク |
|---|---|---|---|
| 1 | Google OAuth 認証（PKCE + ループバック） | `動作確認済み（モック）` / 実機 `未検証` | PKCE `S256`・CSRF `state`・`drive.file` 最小スコープ強制・`127.0.0.1:0` ループバック受信を自動テスト確認。実 Google アカウント認証は `GOOGLE_OAUTH_CLIENT_ID` 未設定のため保留（`--live` で追試可能）。 |
| 2 | フォルダ作成 | `動作確認済み（モック）` / 実機 `未検証` | `FormulaDesk/大学数学` 階層フォルダ作成をモック HTTP サーバーで確認。実機は OAuth 認証待ち。 |
| 3 | ファイルのアップロード（`files.create`） | `動作確認済み（モック）` / 実機 `未検証` | multipart アップロード + `appProperties` 冪等キーによる重複作成防止を確認。実機は OAuth 認証待ち。 |
| 4 | 同一ファイル ID での更新（`files.update`） | `動作確認済み（モック）` / 実機 `未検証` | 同一 `drive_file_id` 維持による更新と `md5Checksum` 不一致による外部変更競合検出を確認。実機は OAuth 認証待ち。 |

---

## 3. フェーズ別実装ステータス

| Phase | Issue | コンポーネント | 実装状態 | 自動テスト (Mock/Unit) | 実サービス接続検証 |
|---|---|---|---|---|---|
| Phase 0 | [#1](https://github.com/sskohei/Theorem/issues/1) | 技術検証 PoC（`poc/`） | **PoC 実装完了（一部実機保留）** | 通過（Rust 6 テスト・clippy/fmt 0 警告・PDF.js/TS 通過） | Codex / Antigravity / Typst / PDF.js 実機済；Claude Code・実 Google Drive 保留 |
| Phase 1 | [#2](https://github.com/sskohei/Theorem/issues/2) | Tauri / React 3 カラム / SQLite / ファイル・PDF 基盤 | **実装・ローカル検証完了** | 通過（Rust 7 テスト・clippy/fmt 0 警告・Vitest 2 ファイル 9 テスト・typecheck/lint/build 通過） | Windows 11 Tauri バイナリビルド＆起動・DB/ログ初期化スモーク確認済 |
| Phase 2 | [#3](https://github.com/sskohei/Theorem/issues/3) | AI アダプター / 問題認識 / 解答生成 / チャット / PDF / バージョン管理 / `Theorem` 改称 | **実装・検証完了** | 通過（Rust `cargo test --lib` 19 テスト［MockAdapter E2E・再起動復旧・Typst/LaTeX 厳格エラー・Keychain 分離・Claude 規約防御・OpenAI strict Schema 変換］・clippy/fmt 0 警告；Vitest 3 ファイル 12 テスト・typecheck/lint/build 通過） | **OpenAI Codex 実機 E2E 通過**（`test_live_codex_full_phase2_workflow` 45.1s：認証確認→画像問題認識→Schema v1 解答生成→Typst PDF 出力→ストリーミングチャット検算）、および Windows 11 `theorem.exe` 起動・DB/ログ初期化スモーク確認済（Antigravity ACP 実機は Phase 0 済・アプリ統合 UI 対話確認は `pnpm.cmd tauri dev` 推奨、Claude Code 実機は未導入のため保留） |
| Phase 3 | [#4](https://github.com/sskohei/Theorem/issues/4) | Science MCP Server / 計算・検証・グラフ / PDF 埋め込み | 未着手 | 未実行 | N/A (ローカル) |
| Phase 4 | [#5](https://github.com/sskohei/Theorem/issues/5) | Google Drive OAuth / 永続ジョブキュー / 更新・競合検出 | 未着手 | 未実行 | 未実施 |
| Phase 5 | [#6](https://github.com/sskohei/Theorem/issues/6) | 部分修正・差分承認 / 答案添削 / 全文検索 / 高度物理図 | 未着手 | 未実行 | 未実施 |
| Phase 6 | [#7](https://github.com/sskohei/Theorem/issues/7) | インストーラー / クロスプラットフォーム / CI・配布 / 日英対応 | 未着手 | 未実行 | N/A |

---

## 4. 未解決の課題・確認待ち事項

1. **実 Google Drive 接続検証（保留）**: ユーザーの Google Cloud Desktop OAuth クライアント ID を用いた `poc/gdrive-poc --live` の実機確認待ち（Phase 4 本実装前までに確認推奨）。
2. **Claude Code 実機接続検証（保留）**: ローカル未導入およびサブスクリプション OAuth のサードパーティ規約制約あり。`ClaudeAdapter` では `official_harness` モードを規約保護のため明示ブロック（`terms_restricted`）し、明示オプトインの `api_key_keyring` モード（`--permission-mode plan --disallowedTools Bash,Write,Edit`）を実装済み。実機確認は `claude` CLI 導入可能環境で追試予定。
3. **Antigravity ACP / 対話 GUI 目視確認**: `AcpAdapter` にてターン開始時のキャンセル後残留チャンクドレイン制御・`agent_thought_chunk` フィルタ・権限要求ブリッジを実装済み。Windows 11 での `theorem.exe` ビルド・起動スモークおよび OpenAI Codex 実機 E2E は通過済みだが、WebView2 上での Antigravity ACP 対話チャット・PDF 領域切り出し・OS D&D の目視操作確認は `pnpm.cmd tauri dev` での最終確認を推奨。
4. **配布戦略（Phase 6）**: Windows エンドユーザー向けに Python (`uv`) / Science MCP 環境および SIL OFL 日本語フォントをどう同梱・初期化するか Phase 6 で確定する。（なお旧識別子 `dev.formuladesk.app` の `.app` 接尾辞警告は `dev.theorem.desktop` への改称により解消済み。）

---

## 関連ドキュメント

- [Phase 0 PoC レポート（poc/README.md）](../poc/README.md)
- [開発ロードマップ（roadmap.md）](./roadmap.md)
- [プロバイダー互換性と認証条件（provider-compatibility.md）](./provider-compatibility.md)
- [ADR 一覧（adr/README.md）](./adr/README.md)
