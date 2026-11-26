# AGENTS.md — Theorem（旧称: FormulaDesk）AI エージェント向け開発ガイド

本ファイルは、このリポジトリで設計・実装・検証・レビューを行うすべての AI コーディングエージェント（Codex / Claude Code / Antigravity 等）が遵守すべきルール、アーキテクチャ不変条件、セキュリティ制約、および開発ワークフローを定義します。

---

## 1. プロジェクト概要と正本ドキュメント

- **プロジェクト**: **Theorem**（旧称: FormulaDesk）— 大学数学・大学物理の学習に特化した OSS・ローカルファーストのデスクトップアプリ（Windows 11 最優先、将来的に macOS / Linux 対応）。
- **開発指示書**: [prompts/mvp.md](./prompts/mvp.md)
- **現在地**: Phase 0（技術検証 PoC）・Phase 1（アプリ基盤 `#2`）・Phase 2（AI 機能統合 `#3`）・Phase 3（Science MCP Server `#4`）実装・検証完了、Phase 4（Google Drive 連携 `#5`）着手待ち。最新の実装・検証状況は必ず [docs/implementation-status.md](./docs/implementation-status.md) を確認すること。

### 作業前に参照すべきドキュメント

| 作業内容 | 必読ドキュメント |
|---|---|
| 全体方針・要件・フェーズ確認 | [docs/product-specification.md](./docs/product-specification.md), [docs/requirements.md](./docs/requirements.md), [docs/roadmap.md](./docs/roadmap.md) |
| アーキテクチャ・IPC・ディレクトリ構成 | [docs/architecture.md](./docs/architecture.md), [docs/adr/README.md](./docs/adr/README.md) |
| SQLite スキーマ・マイグレーション | [docs/database.md](./docs/database.md) |
| AI ハーネス統合・認証・規約 | [docs/agent-integration.md](./docs/agent-integration.md), [docs/provider-compatibility.md](./docs/provider-compatibility.md) |
| Science MCP Server（Python） | [docs/science-mcp.md](./docs/science-mcp.md) |
| Typst PDF 生成・LaTeX 変換 | [docs/pdf-generation.md](./docs/pdf-generation.md) |
| Google Drive OAuth・ジョブキュー | [docs/google-drive.md](./docs/google-drive.md) |
| セキュリティ・権限境界・CSP | [docs/security.md](./docs/security.md) |
| React UI・Zustand ストア | [docs/ui-design.md](./docs/ui-design.md) |
| テスト戦略・CI・環境設定 | [docs/testing.md](./docs/testing.md), [docs/development.md](./docs/development.md) |

---

## 2. アーキテクチャ不変条件（必ず守ること）

1. **責務分離**:
   - UI（React）とビジネスロジック（Rust `src-tauri/src/`）を分離する。
   - **React から SQLite や OS キーチェーンを直接操作しない**。すべての永続化・ファイル操作・資格情報操作は Rust の検証済み Tauri Command 経由とする。
2. **ローカルファースト・サーバーレス**:
   - 本仕様に存在しない専用バックエンドサーバー、クラウド DB、独自ユーザー登録、端末間同期、SQLite ファイルのクラウド自動同期を導入しない。
   - 外部 AI や Google Drive が利用できないオフライン状態でも、保存済みの問題・解答・PDF・会話履歴が閲覧できる設計を維持する。
3. **ファイルシステム管理**:
   - 原本ファイルの内容は変更しない。
   - ファイル保存は一時ファイル書き込み後のアトミックリネームで行う。
   - DB には絶対パスを保存せず、アプリ管理領域ルートからの相対パスを保存する。
4. **AI アダプター隔離**:
   - ハーネス固有プロトコル（Codex App Server / Claude Code / Antigravity ACP / Custom ACP）は `src-tauri/src/agents/providers/` 内に閉じ込め、上位層は共通の `AgentAdapter` trait・`AgentEvent`・`AgentCapabilities` のみを扱う。
   - 取得できない情報（認証状態・モデル一覧・利用量等）を推測補完せず「未取得」として扱う。
5. **Google Drive と AI の完全分離**:
   - AI エージェントに SQLite 直接アクセス権限、Google Drive 操作権限、および Google Drive 認証情報を一切渡さない。
   - ローカルで問題・解答を削除しても Google Drive 上のファイルを自動削除しない。
   - Google Drive 更新は `document_id + logical_role` で論理対象を管理し、`files.update` により同一 Drive ファイル ID（共有リンク）を維持する。
6. **非破壊バージョン管理**:
   - 解答修正時に過去バージョンを上書きしない。チャットからの部分修正は必ず差分提示とユーザー承認を経て新バージョンを作成する。

---

## 3. セキュリティ・コンプライアンス絶対ルール

### 3.1 AI ハーネス利用規約・認証

- 公式ハーネスを改変しない。非公開 API を利用しない。認証トークンを非公式に抽出・中継しない。他クライアントを偽装しない。
- 認証は原則として公式ハーネスに委ねる。
- AI 処理は必ずユーザー自身の明示的な操作を起点とし、利用条件未確認の無人バッチ処理や利用上限回避処理を実装しない。
- **ユーザーの明示的な許可なく従量課金（API キー）へ切り替えない**（例: Codex サブスクリプション実行時は `OPENAI_API_KEY` を子プロセスへ引き継がない）。
- サブスクリプション利用条件が確認できない認証方式（例: Claude Code サードパーティ OAuth 等）は無断で有効化せず、`docs/provider-compatibility.md` に従い確認済みの方式のみ有効化する。

### 3.2 サンドボックス・権限・プロンプトインジェクション防御

- 問題 PDF・画像・参考資料・AI 出力はすべて**信頼できない入力**として扱う。
- エージェント実行ごとに隔離ワークスペース `workspaces/<session-uuid>/` を作成し、作業ディレクトリ指定だけに頼らずハーネス公式サンドボックス・権限フック・環境変数サニタイズを組み合わせる。
- エージェントによる**任意シェル実行はデフォルト禁止**。必須の権限制御を提供できないエージェントは該当機能を無効化する。
- パストラバーサルおよびシンボリックリンク経由の管理領域外アクセスを Rust 側で必ず検証・拒否する。
- Tauri v2 の Capability と CSP は最小権限で設定する（外部スクリプト・`'unsafe-eval'`・Markdown の生 HTML 描画を禁止）。

### 3.3 Science MCP Server（Python）

- **汎用 Python コード実行ツールを絶対に実装しない**。
- `eval()` / `exec()` / 生の `sympy.parse_expr()` 呼び出しを禁止し、Python `ast` モジュールによる厳格なホワイトリスト数式パーサーを使用する。
- 入力サイズ・AST 深さ・行列サイズ・指数上限・ワーカープロセスによる実行タイムアウト・固定 `artifacts/` 出力先（UUID ファイル名）を強制する。
- ユーザーの既存グローバル MCP 設定ファイルを無断で書き換えない。

### 3.4 Typst PDF 生成

- AI が生成した任意の Typst コードを実行しない。JSON Schema 検証済みの構造化解答データのみを厳格エスケープのうえ検証済みテンプレートへ埋め込む。
- LaTeX 数式をそのまま Typst 数式へ埋め込まない。未対応記法を黙って削除したり不正確な数式へ自動置換せず、明示エラーと修正導線を返す。
- コンパイル時の参照ルート制限・オフライン完結（外部パッケージ取得禁止）・タイムアウトを強制する。

### 3.5 機密情報・ログ

- API キー、OAuth トークン、OAuth クライアントシークレット、パスワードを **SQLite に平文保存しない**、**ログへ出力しない**、**Git リポジトリへコミットしない**。
- ログには問題文全文・会話履歴全文も出力しない。認証情報は OS キーチェーンへ保存し、debug / release ビルドでサービス名を分離する。

---

## 4. コーディング規約

### TypeScript / React（`src/`）

- `tsconfig.json` で `strict: true` を維持し、不要な `any` を禁止する。
- コンポーネントとビジネスロジック・IPC 通信を分離する（`services/`, `hooks/`, `stores/`）。
- Rust DTO と整合する型安全な Tauri IPC ラッパーを使用する。
- 不要な巨大グローバルストアを避け、責務単位で Zustand ストアを分割する。
- 初期言語は日本語とし、UI 文字列は翻訳リソース（i18n）で管理する。

### Rust（`src-tauri/`）

- `Result<T, E>` によるエラー処理を徹底し、本番コードでの安易な `.unwrap()` / `.expect()` を禁止する。
- 非同期処理（Tokio）ではキャンセルと子プロセス終了処理（Windows での孤児プロセス防止を含む）を必ず実装する。
- 不要な `unsafe` を使用しない。
- 変更後は `cargo fmt --all -- --check` と `cargo clippy --all-targets -- -D warnings` を必ず通す。

### Python（`science-mcp/`）

- すべての関数・入出力に型ヒントを付与する。
- 依存関係は `uv`（`pyproject.toml` + `uv.lock`）で管理する。
- 変更後は `uv run ruff check`、`uv run ruff format --check`、`uv run pytest` を必ず通す。

### 共通

- 大規模な単一ファイルを避け、モジュール責務を明確にする。
- 不要な依存ライブラリを追加しない。追加前に安定版・互換性・MIT 互換ライセンスを確認する。
- 存在しない API や未確認の仕様を推測で実装しない。
- 重要な技術・設計判断は `docs/adr/` に ADR として記録する。

---

## 5. 開発フェーズ・テスト・誠実報告ルール

### GitHub Issue とフェーズ順序

開発は Phase 0 から順に段階的に進める：

- Phase 0（技術検証 PoC）: `#1`
- Phase 1（アプリ基盤）: `#2`
- Phase 2（AI 機能）: `#3`
- Phase 3（Science MCP Server）: `#4`
- Phase 4（Google Drive 連携・MVP 完成）: `#5`
- Phase 5（高度な学習機能）: `#6`
- Phase 6（OSS 正式公開）: `#7`

### 絶対禁止事項（報告・品質）

- テストを実行せず「成功」「完了」と報告すること。
- UI だけを実装してバックエンド機能が動作するように見せかけること。
- 未実装または未検証のプロバイダー・機能を「動作確認済み」として表示・記載すること。
- **Mock Adapter によるテスト合格を、実際の外部 AI ハーネスや Google Drive 接続の成功と混同すること**。
- 外部認証が必要な検証を、ユーザーの認証情報がない状態で成功扱いにすること（必要な場合は保留として記録し、実行可能な他作業を継続する）。

### 各タスク・フェーズ完了時チェックリスト

1. 実装内容が対象 Issue・仕様・セキュリティ要件を満たしているか確認する。
2. 該当する自動テストを実行する（`cargo test` / `pnpm test` / `uv run pytest`）。
3. フォーマッタ・リンタ・型検査を実行する（`cargo fmt` / `cargo clippy` / `pnpm typecheck` / `pnpm lint` / `ruff`）。
4. 設計変更や進捗に応じて `docs/`・`docs/adr/`・`docs/implementation-status.md` を更新する。
5. 未解決の課題・実サービス検証待ち事項を明示的に記録する。
