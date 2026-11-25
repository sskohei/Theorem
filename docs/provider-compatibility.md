# AI プロバイダー互換性・認証条件マトリクス（Provider Compatibility）

> **ステータス**: 机上調査完了 / Phase 0 実機検証待ち
> **最終更新**: 2026-11-25
> **重要**: 本ドキュメントは開発開始時点の公式公開情報に基づく整理であり、利用規約および認証仕様は変更される可能性がある。Phase 0 実装前およびリリース前に必ず各プロバイダーの最新公式ドキュメント・利用規約を再確認すること。

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

- `動作確認済み`: Phase 0 以降の PoC / E2E テストで実機確認完了
- `公式仕様確認済み（PoC未検証）`: 公式ドキュメントで仕様を確認済みだが、本環境での実機検証は未実施
- `利用条件の確認が必要`: サブスクリプション利用規約・サードパーティクライアント条件の確認が未完了または制約あり
- `未検証`: 技術仕様・動作ともに Phase 0 で検証予定
- `対応不可`: 技術的または規約上の理由によりサポート対象外

---

## 3. プロバイダー別比較マトリクス（Phase 0 検証前）

| 項目 | OpenAI Codex | Anthropic Claude Code | Google Antigravity | Custom ACP Agent |
|---|---|---|---|---|
| **公式ハーネス** | Codex CLI (`codex app-server`) | Claude Code CLI / Agent SDK | Antigravity CLI (`agy acp`) | 任意の ACP 準拠 CLI |
| **推奨接続方式** | Codex App Server (JSON-RPC 2.0 / `stdio`) | 公式プロセス / Agent SDK / ACP アダプター（要確認） | 公式 ACP (JSON-RPC 2.0 / `stdio`) | 標準 ACP (JSON-RPC 2.0 / `stdio`) |
| **技術仕様調査状況** | 公式仕様確認済み（PoC未検証） | 公式仕様確認済み（PoC未検証） | 公式仕様確認済み（PoC未検証） | 公式仕様確認済み（PoC未検証） |
| **サブスクリプション認証** | ChatGPT Plus / Pro / Business 等（`codex login` / Sign in with ChatGPT） | **⚠️ 規約上の重要制約あり（下記詳細参照）** | Google AI Free / Pro / Ultra / Business（`agy acp --login`、利用形態条件あり） | エージェント依存 |
| **API キー代替** | `OPENAI_API_KEY`（明示選択時のみ） | `ANTHROPIC_API_KEY`（明示選択時のみ・サードパーティ標準） | `GEMINI_API_KEY` / GCP Vertex AI（明示選択時のみ） | エージェント依存 |
| **画像入力** | 未検証（Phase 0） | 未検証（Phase 0） | 未検証（Phase 0） | Capability 動的判定 |
| **ストリーミング** | 未検証（Phase 0） | 未検証（Phase 0） | 未検証（Phase 0） | Capability 動的判定 |
| **セッション再開** | 未検証（Phase 0） | 未検証（Phase 0） | 未検証（Phase 0） | Capability 動的判定 |
| **処理キャンセル** | 未検証（Phase 0） | 未検証（Phase 0） | 未検証（Phase 0） | Capability 動的判定 |
| **MCP 登録** | 未検証（Phase 0） | 未検証（Phase 0） | 未検証（Phase 0） | Capability 動的判定 |
| **権限制御 / サンドボックス** | 未検証（Phase 0） | 未検証（Phase 0） | 未検証（Phase 0） | Capability 動的判定 |
| **MVP における優先位置づけ** | 候補（Phase 0 実機・規約検証で判定） | 候補（API キーモード優先 / サブスク規約確認必須） | 候補（ACP クライアント規約・実機検証で判定） | 拡張対応 |

---

## 4. プロバイダー別詳細調査・利用条件整理

### 4.1 OpenAI Codex（`codex app-server`）

- **接続仕様**:
  - `codex app-server` を子プロセス起動し、改行区切り JSON-RPC 2.0 over `stdio` で双方向通信を行う。
  - `initialize` ハンドシェイク、スレッド/ターン管理、ストリーミングイベント、ツール/承認フローをサポート。
- **認証仕様**:
  - ユーザーが事前に `codex login` を実行すると、ローカル認証ストア（`~/.codex/auth.json` 等）に OAuth セッションが保存される。
  - 認証状態は `codex login status` で確認可能。
  - **誤課金防止設計**: サブスクリプション利用プロファイル選択時は、子プロセスへ環境変数 `OPENAI_API_KEY` を引き継がない（意図せず従量課金 API へフォールバックすることを防ぐ）。
- **利用規約・コンプライアンス上の確認事項**:
  - OpenAI の *Using your ChatGPT plan in other apps and sites*、*Sign in with ChatGPT Terms*、*OpenAI Terms of Use* / *Service Terms* を参照する。
  - パスワード収集・トークン抽出・利用上限回避は禁止。認証は完全に公式 `codex login` に委ね、ユーザー操作起点の対話的学習用途として利用する。
  - **ステータス**: `利用条件の確認が必要`（数学・物理学習用デスクトップクライアントからローカル `codex app-server` を対話利用する形態の適合性を Phase 0 で最終確認する）。

### 4.2 Anthropic Claude Code

- **接続仕様**:
  - 公式 Claude Code CLI、Claude Agent SDK、または ACP アダプター経由の接続が技術的候補となる。
- **⚠️ サブスクリプション利用規約に関する重要所見**:
  - 公開ドキュメントおよび規約（Anthropic Consumer Terms of Service / Claude Code *Legal and compliance*）では、**Claude Free / Pro / Max / Team / Enterprise のサブスクリプション OAuth トークンは、公式の未改変 Claude Code および Claude.ai のネイティブ利用専用**とされている。
  - サードパーティ製アプリや外部エージェントハーネス・Agent SDK において、ユーザーのサブスクリプション OAuth 認証情報を抽出・中継・流用することは規約違反とされ、サーバー側でもブロック対象となり得る。
  - 一方、サードパーティ製品や Agent SDK 統合では **Claude Console の API キー（従量課金）** の利用が公式に案内されている。
- **FormulaDesk での対応方針**:
  1. **サブスクリプション OAuth トークンの抽出・中継は絶対に実装しない**。
  2. Claude Code のサブスクリプション認証による外部クライアント統合は、Phase 0 で最新公式ドキュメント（`code.claude.com/docs/en/legal-and-compliance`）を再確認し、明確に許可されている形態が確認できない限り**サブスクリプション自動連携は無効（`利用条件の確認が必要`）**とする。
  3. ユーザーが明示的に同意し API キーを設定した場合の API キー認証モード（OS キーチェーン保存）を、許可された正規の代替手段として位置づける。決して無断で従量課金へ切り替えない。

### 4.3 Google Antigravity（`agy acp`）

- **接続仕様**:
  - Antigravity CLI（`agy`）は公式に **Agent Client Protocol (ACP)** サーバー（`agy acp`）を内蔵しており、JSON-RPC 2.0 over `stdio` でクライアントと通信する。
  - Rust の `agent-client-protocol` クレートを用いたクライアント統合と親和性が高い。
- **認証仕様**:
  - ローカルの `agy` 認証（`agy acp --login` 等）を継承する。
  - 個人向け（Free / Google AI Pro / Google AI Ultra）のほか、企業/GCP 向け（Vertex AI / Agent Platform）や `GEMINI_API_KEY` 経路が存在する。
- **利用規約・コンプライアンス上の確認事項**:
  - 公式エディタや薄い対話的 ACP クライアント（Antigravity が中核エージェントとして動作し、ユーザー操作起点で対話する形態）での利用は ACP の本来の用途である。
  - 一方、個人コンシューマープラン（`oauth-personal`）を用いた**無許可プロキシ化・システムプロンプトの改変・シリアライズ履歴の自動リプレイ・自律バッチループ**は規約違反やアカウント制限のリスクがある。
  - **ステータス**: `利用条件の確認が必要`（対話的 ACP クライアントとしての利用範囲、個人サブスクリプション vs API キー/GCP 連携の境界を Phase 0 で確認する）。

### 4.4 カスタム ACP エージェント（Custom ACP）

- **接続仕様**:
  - ユーザーが任意の ACP 準拠バイナリパスと引数を登録し、`initialize` ハンドシェイクによる Capability Negotiation で動的に機能判定を行う。
- **セキュリティ条件**:
  - FormulaDesk の必須セキュリティ要件（任意シェル禁止・隔離ワークスペース制限・権限フック）を満たせないカスタムエージェントは、安全制御が確認できるまで該当機能（自動ツール実行等）を無効化し、ユーザーへ警告を表示する。

---

## 5. MVP 開発におけるリスク緩和方針

- Codex・Claude Code・Antigravity の 3 種類対応は正式版（Phase 6）の重要要件とする。
- Phase 0 検証の結果、一部のプロバイダーに規約上の制限または技術的障壁が判明した場合でも、**利用条件および動作に問題のない少なくとも 1 つの公式ハーネスを優先採用して MVP 開発を継続する**。
- 未検証または規約未確認のプロバイダーについては、UI 上で「動作確認済み」と誤認させず、正確なステータス（未検証 / API キーのみ対応 / 利用条件確認中など）を表示する。

---

## 6. Phase 0 検証チェックリスト（各プロバイダー共通）

各プロバイダーについて Phase 0 で以下を実機検証し、結果を本ドキュメントおよび [implementation-status.md](./implementation-status.md) へ記録する。

- [ ] 公式バイナリ検出・バージョン取得
- [ ] 公式認証方式の確認（認証情報なしでの疑似成功扱いは禁止）
- [ ] 最新利用規約・サブスクリプション利用条件の記録
- [ ] 基本テキスト応答 + ストリーミング受信
- [ ] 画像入力（問題 PDF ページ PNG からの数式認識）
- [ ] セッション作成および再開（非対応時は挙動記録）
- [ ] 処理キャンセル（途中中断と子プロセス健全性）
- [ ] Science MCP のセッション単位登録・ツール呼び出し
- [ ] 権限制御（任意シェル禁止・ワークスペース外アクセス遮断）

---

## 関連ドキュメント

- [AI ハーネス統合設計（agent-integration.md）](./agent-integration.md)
- [セキュリティ設計（security.md）](./security.md)
- [実装・検証状況（implementation-status.md）](./implementation-status.md)
