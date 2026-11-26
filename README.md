# Theorem

> **Open-source, local-first AI workspace for mathematics and physics.**

**Theorem**（旧称: FormulaDesk）は、大学数学・大学物理の学習に特化した、オープンソースかつローカルファーストなデスクトップアプリケーションです。
PDF や画像形式の問題を取り込むと、ユーザーが既に利用している公式 AI ハーネスと連携して問題文を認識し、途中式・論理的証明・計算検証・図やグラフを含む解答を生成します。生成した解答は Typst で高品質な日本語・数式入り PDF としてローカルに保存でき、問題や解答について AI と継続的に議論・検証・修正できます。

> **現在のプロジェクトステータス**: **Pre-MVP（Phase 0 PoC・Phase 1 アプリ基盤・Phase 2 AI 機能統合完了 / Phase 3 Science MCP Server 着手可能）**
> 仕様書・設計ドキュメント・ADR・Phase 0 PoC（`poc/`）・Phase 1 アプリ基盤・Phase 2 AI 機能（`AgentAdapter` / Codex・Antigravity・Custom ACP・Claude Code 規約制御・MockAdapter / 問題認識・編集 / Schema v1 解答生成 / Embedded Typst 0.15.1 + `mitex` PDF 生成 / 非破壊バージョン管理 / AI チャット・ストリーミング・権限要求カード）を実装・検証済みです。最新の進捗と検証状況は [docs/implementation-status.md](./docs/implementation-status.md) を参照してください。

---

## 主なコンセプトと方針

- **ローカルファースト**: 問題ファイル・構造化解答・会話履歴・計算検証履歴の正本をすべてローカル（SQLite + ファイルシステム）に保存します。オフライン時でも保存済みデータを閲覧できます。
- **専用サーバー・独自ユーザー登録不要**: アプリ独自のクラウド DB やユーザーアカウント管理を持ちません。
- **既存公式 AI ハーネス活用・プロバイダー非依存**: OpenAI Codex（Codex App Server）、Anthropic Claude Code、Google Antigravity（公式 ACP）、カスタム ACP エージェントを共通アダプター層で統合します。ユーザー自身の既存 AI サブスクリプション利用を優先しつつ、各プロバイダーの利用規約・認証仕様を厳守します。
- **大学数学・大学物理への専門対応**: 大学数学 14 分野・大学物理 10 分野を想定し、定義・定理・証明、物理量・座標系・仮定・次元確認・極限考察などを構造化 JSON スキーマで管理します。
- **Science MCP Server による安全な計算・検証・可視化**: Python（`uv`）製のローカル MCP サーバーで SymPy / NumPy / SciPy / Pint / Matplotlib による数式検算・単位変換・次元解析・グラフ生成を提供します（任意 Python コード実行は完全に禁止）。
- **Typst による高品質 PDF 生成**: TeX Live のインストールを要求せず、検証済みテンプレートと LaTeX → Typst 変換層により日本語・数式・ベクターグラフを含む A4 解答 PDF を出力します。
- **Google Drive エクスポート（任意接続）**: OAuth 2.0 + PKCE と最小権限スコープ（`drive.file`）で問題ファイルと最新解答 PDF をアップロードし、同一 Drive ファイル ID のまま更新（共有リンク維持）できます。※端末間同期や SQLite の自動同期は行いません。

---

## 対応 OS

| OS | 対応優先度 |
|---|---|
| **Windows 11** | 最優先（MVP ターゲット） |
| **macOS** | 将来的に正式対応（Phase 6 で検証） |
| **Linux** | 将来的に正式対応（Phase 6 で検証） |

---

## 採用技術スタック

| 領域 | 採用技術 |
|---|---|
| デスクトップ基盤 | Tauri v2 |
| フロントエンド | React / TypeScript / Vite / pnpm / Tailwind CSS / shadcn/ui / Lucide / Zustand |
| PDF 表示・数式・Markdown | PDF.js / KaTeX / react-markdown |
| バックエンド | Rust / Tokio / reqwest / SQLite (`sqlx`) / serde |
| AI 通信・ツール連携 | ACP (`agent-client-protocol`) / Codex App Server / Agent SDK / MCP |
| 科学計算 | Python (`uv`) / MCP Python SDK / SymPy / NumPy / SciPy / Pint / Matplotlib |
| PDF 組版 | Typst + LaTeX→Typst 変換層 |
| クラウド連携・認証 | Google Drive API v3 / OAuth 2.0 + PKCE / OS キーチェーン |
| テスト・CI | `cargo test` / Vitest / `pytest` / GitHub Actions |

---

## システム構成概要

```text
Theorem
├── React + TypeScript UI         # 3カラム学習UI（ライブラリ / 問題・解答ビューア / AIチャット / 設定）
├── Tauri / Rust Core             # SQLite / ローカルストレージ / Agent Manager / PDF生成 / Drive連携 / セキュリティ
├── External AI Harnesses         # Codex App Server / Claude Code / Antigravity ACP / Custom ACP
├── Science MCP Server (Python)   # SymPy / NumPy / SciPy / Pint / Matplotlib（AST安全パーサー・タイムアウト隔離）
└── External Services             # AIプロバイダー（公式ハーネス経由） / Google Drive API v3（任意）
```

詳細は [docs/architecture.md](./docs/architecture.md) を参照してください。

---

## 開発ロードマップと GitHub Issues

| Phase | 名称 | 区分 | Issue | ステータス |
|---|---|---|---|---|
| **Phase 0** | 技術検証（PoC）：AI ハーネス・Typst PDF・Google Drive | PoC / MVP | [#1](https://github.com/sskohei/Theorem/issues/1) | PoC 完了（一部外部認証保留） |
| **Phase 1** | アプリ基盤：Tauri・React 3 カラム UI・SQLite・問題取り込みとビューア | MVP | [#2](https://github.com/sskohei/Theorem/issues/2) | 実装・ローカル検証完了 |
| **Phase 2** | AI 機能：エージェント管理・問題認識・解答生成・チャット・PDF・バージョン管理 | MVP | [#3](https://github.com/sskohei/Theorem/issues/3) | 実装・検証完了 |
| **Phase 3** | 数学・物理計算ツール：Science MCP Server・検証・次元解析・グラフ埋め込み | MVP | [#4](https://github.com/sskohei/Theorem/issues/4) | 未着手 |
| **Phase 4** | Google Drive 連携：OAuth PKCE・アップロード・同一 ID 更新・永続ジョブキュー | MVP | [#5](https://github.com/sskohei/Theorem/issues/5) | 未着手 |
| **Phase 5** | 高度な学習機能：解答部分修正・差分承認・答案添削・全文検索・高度物理図 | Post-MVP | [#6](https://github.com/sskohei/Theorem/issues/6) | 未着手 |
| **Phase 6** | OSS 正式公開：インストーラー・クロスプラットフォーム・配布・CI/CD・日英対応 | Release | [#7](https://github.com/sskohei/Theorem/issues/7) | 未着手 |

---

## ドキュメント一覧

### 仕様・設計ドキュメント

- [製品仕様書（docs/product-specification.md）](./docs/product-specification.md)
- [要件定義書（docs/requirements.md）](./docs/requirements.md)
- [システムアーキテクチャ（docs/architecture.md）](./docs/architecture.md)
- [データベース設計（docs/database.md）](./docs/database.md)
- [AI ハーネス統合設計（docs/agent-integration.md）](./docs/agent-integration.md)
- [AI プロバイダー互換性・認証条件（docs/provider-compatibility.md）](./docs/provider-compatibility.md)
- [Science MCP Server 仕様（docs/science-mcp.md）](./docs/science-mcp.md)
- [PDF 生成・組版設計（docs/pdf-generation.md）](./docs/pdf-generation.md)
- [Google Drive 連携仕様（docs/google-drive.md）](./docs/google-drive.md)
- [セキュリティ・プライバシー設計（docs/security.md）](./docs/security.md)
- [UI / UX 設計（docs/ui-design.md）](./docs/ui-design.md)
- [テスト方針（docs/testing.md）](./docs/testing.md)
- [開発者向けガイド（docs/development.md）](./docs/development.md)
- [開発ロードマップ（docs/roadmap.md）](./docs/roadmap.md)
- [実装・技術検証状況（docs/implementation-status.md）](./docs/implementation-status.md)
- [Architecture Decision Records（docs/adr/）](./docs/adr/README.md)

### AI エージェント向け開発ガイド

- [AGENTS.md](./AGENTS.md)

---

## 開発環境とセキュリティ上の注意

- 開発前提環境（Rust / Node.js + pnpm / Python + `uv` / MSVC Build Tools 等）およびコーディング規約は [docs/development.md](./docs/development.md) を参照してください。
- **プライバシー**: ローカルファーストは完全オフラインを意味しません。クラウド AI プロバイダーを利用する場合、問題画像や選択したコンテキストは外部 AI サービスへ送信されます。
- **認証情報と利用規約**: 公式 AI ハーネスの改変・非公開 API 利用・認証トークンの非公式抽出は行いません。また、API キーや OAuth クライアント情報をリポジトリへコミットしないでください。

> ※英語版 README と `README.ja.md` への分離、およびアプリ UI の英語対応は Phase 6（[#7](https://github.com/sskohei/Theorem/issues/7)）で整備予定です。

---

## ライセンス

[MIT License](./LICENSE)
