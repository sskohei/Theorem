# 開発者向けガイド（Development）

> **ステータス**: Draft v1.0（Phase 1 のスキャフォールディング後に実コマンドを確定・更新予定）
> **最優先開発 OS**: Windows 11（macOS / Linux は将来対応）
> **最終更新**: 2026-11-25

---

## 1. 開発前提環境

### 1.1 必須ツール（Windows 11）

| ツール | 用途 | 確認コマンド |
|---|---|---|
| **Rust** (`rustup` / `cargo`) | Tauri バックエンド・コア実装 | `rustc --version && cargo --version` |
| **Visual Studio C++ Build Tools** | Windows 上での Rust / Tauri ビルド（MSVC ツールチェーン） | Visual Studio Installer で確認 |
| **WebView2 Runtime** | Tauri v2 Windows WebView（Windows 11 は標準搭載） | システム標準 |
| **Node.js (LTS)** + **pnpm** | React / TypeScript / Vite フロントエンド | `node -v && pnpm -v` |
| **Python 3.11+** + **uv** | Science MCP Server の依存管理・テスト | `uv --version` |
| **Git** + **GitHub CLI (`gh`)** | バージョン管理・Issue / PR 運用 | `git --version && gh --version` |
| **Typst CLI**（Phase 0 検証用） | Typst サイドカー案の検証およびテンプレート開発 | `typst --version` |

> 各ライブラリの具体的なバージョンは、実装フェーズ着手時点で利用可能な安定版および互換性を確認して固定する。

### 1.2 任意（実機接続検証用）

外部サービスの接続検証（Phase 0 / Live E2E）を行う場合は、開発者自身が公式手順で以下を準備する。

- **OpenAI Codex CLI**: 公式手順でインストールし `codex login` で認証（`codex login status` で確認）
- **Anthropic Claude Code**: 公式手順でインストール（利用規約上の認証条件は [provider-compatibility.md](./provider-compatibility.md) を参照）
- **Google Antigravity CLI (`agy`)**: 公式手順でインストールし `agy acp --login` 等で認証
- **Google Cloud Desktop OAuth クライアント**: Google Drive API を有効化し、`https://www.googleapis.com/auth/drive.file` スコープのみでデスクトップアプリ用 OAuth クライアントを作成

> ⚠️ **厳禁事項**: API キー、OAuth クライアントシークレット、アクセストークン、ユーザー認証情報を**絶対にリポジトリへコミットしないこと**。

---

## 2. リポジトリセットアップと想定開発コマンド

※以下は Phase 1〜Phase 3 で各コンポーネントを構築した後の標準ワークフローである。現時点（Phase 0 着手前）では `docs/` と `prompts/mvp.md` のみが存在する。

### 2.1 フロントエンド + Tauri アプリ起動（Phase 1〜）

```powershell
# 依存関係インストール
pnpm install

# Tauri 開発モード起動（Vite + Rust ホットリロード）
pnpm tauri dev

# フロントエンド型チェック・Lint・テスト
pnpm typecheck
pnpm lint
pnpm test
```

### 2.2 Rust バックエンド検証（Phase 1〜）

```powershell
Set-Location src-tauri
cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings
cargo test
```

### 2.3 Science MCP Server（Phase 3〜）

```powershell
Set-Location science-mcp
uv sync
uv run ruff check
uv run ruff format --check
uv run pytest
```

---

## 3. Google Drive 開発用 OAuth 設定手順

1. Google Cloud Console で開発用プロジェクトを作成し、**Google Drive API** を有効化する。
2. **OAuth 同意画面** を構成する（External の場合はテストユーザーに自分のアカウントを追加）。
3. スコープには `https://www.googleapis.com/auth/drive.file` **のみ**を追加する（`drive` や `drive.readonly` は追加しない）。
4. **認証情報 → OAuth クライアント ID を作成 → アプリケーションの種類: デスクトップ アプリ** を選択する。
5. 取得したクライアント情報は `.env.local`（`.gitignore` 対象）または開発環境の環境変数経由で読み込み、ソースコードや設定ファイルへ直書きしない。

---

## 4. コーディング規約

### 4.1 TypeScript / React

- `tsconfig.json` で **`strict: true`** を有効にする。
- 不要な `any` を禁止する（やむを得ない境界では `unknown` + 型ガードを使用）。
- UI コンポーネントとビジネスロジック・IPC 通信を分離する（`services/`, `hooks/`, `stores/`）。
- Tauri IPC は型安全なラッパー経由で呼び出し、SQLite へ直接アクセスしない。
- 不要な巨大グローバル状態を作らず、機能ごとに Zustand ストアを分割する。
- すべての非同期呼び出しにエラーハンドリングとユーザー向け表示を実装する。

### 4.2 Rust

- すべての失敗し得る処理で **`Result<T, E>`** を使用する。
- 本番コードでの安易な `.unwrap()` / `.expect()` を禁止する（テストコード内を除く）。
- モジュールごとの責務分離を維持し、大規模な単一ファイルを避ける。
- Tokio 非同期処理ではキャンセル（`CancellationToken` 等）と子プロセス終了処理（Graceful shutdown / 孤児防止）を必ず考慮する。
- `cargo fmt` と `cargo clippy --all-targets -- -D warnings` を常にグリーンに保つ。
- 不要な `unsafe` を使用しない。

### 4.3 Python（Science MCP）

- すべての関数・ツール入出力に型ヒント（Type Hints）を付与する。
- パッケージ管理は **`uv`**（`pyproject.toml` + `uv.lock`）で統一する。
- 静的解析・フォーマットは **`Ruff`** を使用する。
- すべてのツールおよびセキュリティ境界（不正数式・タイムアウト）を **`pytest`** で検証する。
- 入力値検証（型・サイズ・AST ホワイトリスト）を徹底する。

### 4.4 共通ルール

- ハードコードされた認証情報を一切使用しない。
- 不要な依存ライブラリを追加しない。追加時はライセンス（MIT 互換性）を確認する。
- 重要な設計・技術選定の判断は `docs/adr/` に ADR として記録する。
- 未実装機能を完成済みと見せかけない。モックテスト成功と実サービス接続検証を区別する。

---

## 5. フェーズ実行・完了時チェックリスト

各フェーズ（Phase 0〜6）および主要な PR の完了時には、必ず以下を実施する。

- [ ] 実装内容が対象 Issue・仕様書・セキュリティ要件を満たしているか確認する
- [ ] 自動テストを実行し、実際の実行結果を確認する（`cargo test` / `pnpm test` / `uv run pytest`）
- [ ] lint と format を実行する（`cargo fmt` / `cargo clippy` / `pnpm lint` / `ruff`）
- [ ] 設計ドキュメント・ADR・[implementation-status.md](./implementation-status.md) を最新の実装・検証状況へ更新する
- [ ] 未解決の課題・制約・外部検証待ち事項を記録する
- [ ] 次のフェーズの実装計画と依存関係を確認する

---

## 関連ドキュメント

- [システムアーキテクチャ（architecture.md）](./architecture.md)
- [テスト方針（testing.md）](./testing.md)
- [開発ロードマップ（roadmap.md）](./roadmap.md)
- [ADR 一覧（adr/README.md）](./adr/README.md)
