# FormulaDesk Phase 0 — 技術検証（PoC）

本ディレクトリには、Issue [#1 (Phase 0: 技術検証 PoC)](https://github.com/sskohei/Theorem/issues/1) に基づく実行可能な PoC コード群を格納する。

---

## パッケージ構成と実行方法

| ディレクトリ | 検証対象 | 実行・テストコマンド（Windows PowerShell） |
|---|---|---|
| `poc/harness-poc/` | Codex App Server / Antigravity ACP / Custom ACP / MCP / 隔離・キャンセル・権限制御 | `cargo test --manifest-path poc/harness-poc/Cargo.toml`<br/>`cargo run --manifest-path poc/harness-poc/Cargo.toml -- custom-acp`<br/>`cargo run --manifest-path poc/harness-poc/Cargo.toml -- codex`<br/>`cargo run --manifest-path poc/harness-poc/Cargo.toml -- antigravity` |
| `poc/mock-acp-agent/` | Custom ACP モックサーバー・テスト用 Science MCP サーバー・問題画像生成 | `python poc/mock-acp-agent/generate_test_image.py` |
| `poc/pdf-typst-poc/` | Embedded Typst 0.15.1・日本語組版・`mitex` vs `tex2typst-rs` 比較・SVG/PNG 埋め込み・インジェクション防御 | `cargo run --manifest-path poc/pdf-typst-poc/Cargo.toml`<br/>`cargo test --manifest-path poc/pdf-typst-poc/Cargo.toml` |
| `poc/pdfjs-poc/` | PDF.js 5.7.284 ビューア UI および生成 PDF の自動テキスト抽出・ページ検証 | `pnpm --dir poc/pdfjs-poc install`<br/>`pnpm --dir poc/pdfjs-poc build`<br/>`node poc/pdfjs-poc/scripts/verify_pdfjs.mjs poc/pdf-typst-poc/output/sample_solution.pdf` |
| `poc/gdrive-poc/` | Google OAuth 2.0 PKCE + ループバック・`drive.file` 最小スコープ・フォルダ作成・`files.create`・同一 ID `files.update`・外部変更競合検出 | モック検証: `cargo test --manifest-path poc/gdrive-poc/Cargo.toml`<br/>実機検証（要環境変数 `GOOGLE_OAUTH_CLIENT_ID`）: `cargo run --manifest-path poc/gdrive-poc/Cargo.toml -- --live` |

---

## Phase 0 実測検証結果サマリー（2026-11-25 / Windows 11）

### 1. AI ハーネス PoC

- **OpenAI Codex (`codex-cli 0.172.0` / `codex app-server --stdio`)** — **実機動作確認済み**
  - `OPENAI_API_KEY` を子プロセスから明示除去した状態で `account/read` → `auth_type: "chatgpt"`（ChatGPT サブスクリプション認証）を確認、`model/list` で 10 モデル取得。
  - `-c mcp_servers.poc_science...` により `~/.codex/config.toml` を一切変更せず（前後 SHA-256 一致）セッションスコープ MCP 登録（`mcpServerStatus/list`）を確認。
  - ストリーミング（24 デルタ）、画像入力（`localImage` で `2X + 3 = 11` → `x = 4` 正解）、`thread/resume` セッション再開、`turn/interrupt` キャンセル（`status=interrupted`）、`sandbox: "read-only"` + `approvalPolicy: "untrusted"` 下でのコマンド実行承認要求拒否（`decision: "decline"`）のすべてを実機確認。
- **Google Antigravity (`agy 1.5.12` / `agy acp --stdio --sandbox --disable-telemetry`)** — **実機動作確認済み（課題あり）**
  - `initialize`（`loadSession: true`, `promptCapabilities.image: true`, `authMethods` 4 種）、`session/new`（`mcpServers` 注入）、ストリーミング、base64 PNG 画像入力（`2x + 3 = 11 => x = 4`）、`session/load` 再開、`session/request_permission` 拒否を実機確認。
  - **発見した技術課題**: `session/cancel` 送信後に先行ターンのバッファ済み `session/update` チャンクが非同期に残留し得るため、Phase 2 の `AgentManager` ではターン単位のイベント相関・ドレイン制御を組み込む必要がある。
- **Custom ACP (`poc/mock-acp-agent/acp_echo_agent.py`)** — **自動テスト動作確認済み（モック）**
  - 隔離ワークスペース・環境変数サニタイズ・Windows Job Object（`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`）・ストリーミング・画像入力・`session/load`・`session/cancel`・`session/request_permission` 拒否が `cargo test` で通過。
- **Anthropic Claude Code** — **未検証（保留）**
  - 本検証環境に `claude` CLI 未インストール。また Consumer ToS 上、サードパーティクライアントからのサブスクリプション OAuth 流用制約があるため、API キー利用または公式規約再確認が必要（[docs/provider-compatibility.md](../docs/provider-compatibility.md)）。

### 2. PDF / Typst + PDF.js PoC

- **Embedded Typst (`typst 0.15.1` + `typst-pdf 0.15.1` + `typst-assets 0.15.1`)** — **実機動作確認済み**
  - カスタム `SandboxedWorld` により完全オフライン（`@preview/*` 拒否）かつ任意ファイル参照不可（`#read("Cargo.toml")` 拒否確認済み）で、A4 3 ページ（日本語 `Noto Serif JP` + 大学数学・物理 12 数式 + Cauchy-Schwarz 証明 + SVG ベクター図 + PNG ラスター図）を **コンパイル 214 ms + PDF 出力 207 ms（計 421 ms / 129 KB）** で生成。
- **LaTeX → Typst 変換比較（大学数学・物理 12 数式 + 未対応マクロ 1 件）**:
  - `mitex 0.2.4`: 生出力 `6/12` → Typst 0.15 シンボル変換 + オフライン互換プレリュード併用で **`12/12`（100%）**。`pmatrix` 括弧・`cases` 波括弧を正しく保持し、未対応マクロ（`\totallyUnknownCustomMacro`）をパース段階で即座にエラー検出。
  - `tex2typst-rs 0.4.1`: 生出力 `9/12` → 併用で **`12/12`** だが、`pmatrix`/`cases` の区切り記号を `delim: #none` 化する挙動および未対応マクロのバックスラッシュ除去を確認。
  - **発見した技術課題**: `mitex 0.2.4` / `tex2typst-rs 0.4.1` は Typst <=0.13 記法（`diff`, `planck.reduce`, `angle.l`/`angle.r`）を出力するため、Typst 0.15 の `partial`, `planck`, `chevron.l`/`chevron.r` へ正規化する変換層が必要（本 PoC で実装・実証済み）。
- **PDF.js (`pdfjs-dist 5.7.284`)** — **実機動作確認済み**
  - Vite ビルド + TypeScript strict 型チェック通過。生成された 3 ページ A4 PDF（`595.3 x 841.9 pt`）を `verify_pdfjs.mjs` で読み込み、日本語および数式テキスト（`𝑖 ℏ 𝜕 𝜕 𝑡 | 𝜓 ( 𝑡 ) ⟩ = 𝐻 ̂ | 𝜓 ( 𝑡 ) ⟩` 等）の抽出を確認。

### 3. Google Drive PoC

- **モックプロトコル・統合テスト（`poc/gdrive-poc`）** — **動作確認済み（モック）**
  - RFC 7636 PKCE (`S256`)・CSRF `state`・`drive.file` 最小スコープ強制（広範な `drive` スコープ拒否）・`127.0.0.1:0` ループバック受信・フォルダ作成・`files.create`（`appProperties` 冪等再試行）・同一 ID `files.update`・外部変更 `md5Checksum` 競合検出を自動テストで確認。
- **実 Google アカウント接続検証（`--live`）** — **未検証（保留）**
  - ユーザーの Google Cloud Desktop OAuth クライアント ID および実アカウント認証が必要なため保留（モック合格と混同しない）。
