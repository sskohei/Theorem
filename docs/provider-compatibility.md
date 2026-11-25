# AI プロバイダー互換性・認証条件マトリクス（Provider Compatibility）

> **ステータス**: Phase 0 実機検証反映（Codex・Antigravity・Custom ACP 検証済み / Claude Code 未検証）
> **最終更新**: 2026-11-25
> **重要**: 利用規約および認証仕様は変更される可能性があるため、リリース前にも必ず各プロバイダーの最新公式ドキュメント・利用規約を再確認すること。

---

## 1. サブスクリプション認証とコンプライアンスの絶対原則

FormulaDesk では、ユーザー自身の既存 AI サブスクリプション利用を優先しつつも、**すべてのプロバイダーで無条件に許可されているとは仮定しない**。以下を厳守する。

1. 公式ハーネスを改変しない。
2. 非公開 API を利用しない。
3. 認証トークンを非公式に抽出・復号・中継しない。
4. 他のクライアント（User-Agent や公式 IDE 等）を偽装しない。
5. 認証情報を公式の用途以外に流用しない。
6. 認証は原則として公式ハーネスに委ねる。
7. AI 処理は必ずユーザー自身の明示的な操作（ボタン押下・メッセージ送信）を起点とする。
8. 利用条件が確認できない無人バッチ処理・自動ループ処理は実装しない。
9. 利用上限・レート制限を回避する処理を実装しない。
10. API キーによる従量課金は任意の代替手段として扱い、**ユーザーの明示的な許可なく従量課金へ切り替えない**。
11. 利用条件が確認できない認証方式については、本ドキュメントに明記し、**確認・許可された認証方式のみを有効化する**。

---

## 2. ステータス凡例

- `動作確認済み（実機）`: Phase 0 PoC で実インストール済み公式ハーネスにより実機確認完了
- `動作確認済み（モック）`: モックエージェントによるプロトコル・自動テスト確認完了（実機接続とは明確に区別）
- `利用条件の確認が必要`: サブスクリプション利用規約・サードパーティクライアント条件の確認が継続必要または制約あり
- `未検証`: 実機環境未導入または認証情報未提供のため未検証
- `対応不可`: 技術的または規約上の理由によりサポート対象外

---

## 3. プロバイダー別比較マトリクス（Phase 0 検証反映）

| 項目 | OpenAI Codex | Anthropic Claude Code | Google Antigravity | Custom ACP Agent |
|---|---|---|---|---|
| **検証バージョン** | `codex-cli 0.172.0` | 未導入（ローカル未検出） | `agy 1.5.12` | `poc/mock-acp-agent` |
| **接続方式** | `codex app-server --stdio` (JSON-RPC 2.0) | 公式 CLI / Agent SDK / ACP（未検証） | `agy acp --stdio --sandbox` (公式 ACP) | 標準 ACP (JSON-RPC 2.0 / `stdio`) |
| **起動・初期化** | 動作確認済み（実機） | 未検証 | 動作確認済み（実機） | 動作確認済み（モック） |
| **公式認証確認** | 動作確認済み（実機：`account/read` → `chatgpt`、`OPENAI_API_KEY` 非継承確認） | 未検証 | 動作確認済み（実機：`authMethods` 4 種返却・既存認証継承） | 該当なし（エージェント依存） |
| **サブスクリプション利用条件** | 利用条件の確認が必要（Sign in with ChatGPT Terms / ToS 継続確認、対話利用限定） | **⚠️ 規約上の重要制約あり（サブスク OAuth のサードパーティ流用禁止・API キー代替推奨）** | 利用条件の確認が必要（公式対話 ACP 利用可／個人プランでの無許可プロキシ・自動ループ禁止） | エージェント依存 |
| **モデル一覧取得** | 動作確認済み（実機：`model/list` で 10 件取得） | 未検証 | 要確認（ACP 拡張またはモデル指定） | Capability 動的判定 |
| **画像入力** | 動作確認済み（実機：`localImage` で数式 PNG 解読成功） | 未検証 | 動作確認済み（実機：base64 PNG ブロックで数式 PNG 解読成功） | 動作確認済み（モック） |
| **ストリーミング** | 動作確認済み（実機：`item/agentMessage/delta` 24 デルタ） | 未検証 | 動作確認済み（実機：`session/update` 受信） | 動作確認済み（モック） |
| **セッション再開** | 動作確認済み（実機：`thread/resume` 成功） | 未検証 | 動作確認済み（実機：`loadSession: true` + `session/load` 成功） | 動作確認済み（モック） |
| **処理キャンセル** | 動作確認済み（実機：`turn/interrupt` → `status=interrupted`） | 未検証 | 動作確認済み（実機・要対策：`session/cancel` 後の残留バッファチャンク対策必要） | 動作確認済み（モック：`stopReason=cancelled`） |
| **セッション単位 MCP 登録** | 動作確認済み（実機：`-c mcp_servers...` で `~/.codex/config.toml` 非改変を SHA-256 確認） | 未検証 | 動作確認済み（実機：`session/new` の `mcpServers` 引数で受理） | 動作確認済み（モック） |
| **権限制御 / サンドボックス** | 動作確認済み（実機：`sandbox="read-only"`, `approvalPolicy="untrusted"` で実行要求を `decline`） | 未検証 | 動作確認済み（実機：`--sandbox` + `session/request_permission` をクライアント側で拒否） | 動作確認済み（モック） |
| **MVP における優先位置づけ** | **MVP 第一優先候補（実機全機能確認済み）** | Phase 2 以降（API キー代替中心・規約確認継続） | **MVP 第二候補（実機確認済み・キャンセル残留対策要）** | 拡張対応（汎用 ACP 基盤共用） |

---

## 4. プロバイダー別詳細・Phase 0 実機所見

### 4.1 OpenAI Codex（`codex app-server --stdio`）

- **Phase 0 実機所見**:
  - `codex app-server generate-json-schema` で公式プロトコル v2 スキーマを取得可能であり、推測実装を排除できた。
  - 子プロセス環境から `OPENAI_API_KEY` を完全除去した状態で `account/read` が `chatgpt`（ChatGPT サブスクリプション）を返却し、従量課金への無断フォールバックを確実に防止できた。
  - `-c mcp_servers.poc_science.command=...` により、`~/.codex/config.toml` の SHA-256 ハッシュを一切変化させずにセッション限定 MCP サーバーを登録できた。
  - `sandbox: "read-only"` + `approvalPolicy: "untrusted"` 設定下では、シェル実行要求時に `item/commandExecution/requestApproval` がクライアントへ届き、`{"decision": "decline"}` でコマンド実行を安全に阻止できた。
  - `turn/start` は `outputSchema`（JSON Schema 強制）にも対応しており、Phase 2 の構造化解答生成に極めて有利。
- **規約・コンプライアンス上の継続注意**:
  - OpenAI の *Sign in with ChatGPT Terms* および *Terms of Use* に従い、ユーザー明示操作起点の対話的学習利用に限定し、無人バッチや上限回避を禁止する。

### 4.2 Google Antigravity（`agy acp --stdio --sandbox --disable-telemetry`）

- **Phase 0 実機所見**:
  - `agy 1.5.12` が標準 ACP を内蔵しており、JSON-RPC 2.0 over `stdio` で `initialize` / `session/new` / `session/load` / `session/prompt` / `session/cancel` / `session/request_permission` が動作することを確認。
  - `authMethods` として `oauth-personal` / `oauth-business` / `gemini-api-key` / `agent-platform` が公開されており、Capability Negotiation と親和性が高い。
  - `--sandbox` + クライアント側 `session/request_permission` 拒否によりシェル実行を阻止できた。
  - **発見した技術リスク**: `session/cancel` 送信後、直前ターンの `session/update` 通知がパイプ内に残留して後続ターンへ混入する現象を確認した。Phase 2 の `AgentManager` ではキャンセル後のドレイン待機またはセッション/ストリーム境界の厳格分離を実装する。
- **規約・コンプライアンス上の継続注意**:
  - 個人コンシューマープラン（`oauth-personal`）では薄い対話的 ACP クライアント利用にとどめ、システムプロンプト改変・履歴自動リプレイ・無人ループを行わないこと。

### 4.3 Anthropic Claude Code

- **Phase 0 状況**: 本開発環境には `claude` CLI が未インストールのため実機接続は `未検証`。
- **利用規約上の重要制約**:
  - Anthropic Consumer Terms of Service および Claude Code *Legal and compliance* では、サブスクリプション OAuth トークンは公式未改変 Claude Code / Claude.ai 専用とされ、サードパーティクライアントでの OAuth 流用・中継は禁止されている。
  - したがって FormulaDesk では、サブスクリプション OAuth の無断流用を絶対に実装せず、最新公式規約で明確に許可された方式、またはユーザーの明示的同意に基づく **API キー認証（OS キーチェーン保管）** のみを有効化する方針を維持する。

### 4.4 カスタム ACP エージェント（Custom ACP）

- `poc/mock-acp-agent/acp_echo_agent.py` により、汎用 ACP クライアント（`poc/harness-poc/src/acp.rs`）の初期化・Capability 判定・MCP 渡し・ストリーミング・画像ブロック・再開・キャンセル・権限拒否を自動テスト検証済み。
- 本番実装でも `initialize` 応答に基づき未対応機能・安全制御不可機能を適切に無効化する。

---

## 関連ドキュメント

- [AI ハーネス統合設計（agent-integration.md）](./agent-integration.md)
- [セキュリティ設計（security.md）](./security.md)
- [実装・検証状況（implementation-status.md）](./implementation-status.md)
- [Phase 0 PoC レポート（poc/README.md）](../poc/README.md)
