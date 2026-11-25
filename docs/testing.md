# テスト方針・品質保証設計（Testing）

> **ステータス**: Draft v1.0
> **テスト基盤**: `cargo test`（Rust） / `Vitest`（React・TS） / `pytest`（Python） / GitHub Actions
> **最終更新**: 2026-11-25

---

## 1. 基本方針

1. **テスト未実行の成功報告を禁止する**: すべてのフェーズ・PR において該当レイヤのテストおよび lint/format を実際に実行し、結果を確認する。
2. **モックテストと実外部接続検証の厳格な分離**:
   - AI ハーネスおよび Google Drive をモック化した **Mock Adapter** を用意し、外部認証情報なしで CI 上の単体・統合テストを再現性高く自動実行できるようにする。
   - ただし **モックテストの合格を、実際の AI ハーネス接続や Google Drive 接続の動作確認済みとして扱わない**。実機接続検証の結果は [implementation-status.md](./implementation-status.md) に明確に区別して記録する。
3. **セキュリティ・異常系をテストの第一級市民とする**: プロンプトインジェクション想定の不正数式、パストラバーサル、通信断、競合、処理キャンセル、プロセス異常終了を必ず自動テストでカバーする。

---

## 2. テストレイヤ別計画

### 2.1 Rust バックエンドテスト（`cargo test`）

| 対象領域 | 主なテストケース |
|---|---|
| データベース操作 | マイグレーション適用、外部キー制約、重複検出（SHA-256）、`solutions.is_current` 一意性、トランザクションロールバック、再起動時の中断ジョブ/実行復旧 |
| ファイル管理 | UUID 発行、SHA-256 計算、アトミック保存、原本不変確認、相対パス解決、孤児ファイル検出 |
| セキュリティ関連 | パストラバーサル拒否（`../`, 絶対パス混入）、シンボリックリンク脱出拒否、ログの秘匿情報マスキング（API キー・OAuth トークン・問題全文除外）、環境変数サニタイズ |
| エージェントアダプター・JSON-RPC | モック子プロセスとの JSON-RPC 2.0 over `stdio` 往復、イベント正規化（`AgentEvent` 12 種）、Capability Negotiation、セッション作成・再開・切替引き継ぎ |
| ストリーミング・キャンセル・異常終了 | ストリーミング途中キャンセル、子プロセス強制終了時のクラッシュ検出と状態更新、利用上限エラー（`rate_limited`）分類 |
| 解答構造化・PDF 生成 | JSON Schema v1 正常/異常検証、LaTeX→Typst 変換（大学数学・物理代表数式）、非対応 LaTeX の黙殺防止エラー、Typst インジェクション文字列のエスケープ、PDF バイナリ生成確認 |
| Google Drive ジョブ管理 | ジョブ状態遷移（7 状態）、重複ジョブ抑止、指数バックオフ計算、`appProperties` 冪等キー修復、外部変更競合検出（`md5Checksum`/`modifiedTime` 不一致）、ローカル削除時の Drive 非削除 |

### 2.2 フロントエンドテスト（`Vitest` + Testing Library）

| 対象領域 | 主なテストケース |
|---|---|
| 共通 UI・レイアウト | 3 カラム開閉・リサイズ、ライト/ダーク切替、日本語リソース表示 |
| Zustand ストア | ライブラリツリー操作、ストリーミングチャンク蓄積、キャンセル状態反映、Drive 同期バッジ更新 |
| ファイル一覧・インポート UI | 科目・フォルダ階層表示、検索・タグ絞り込み、重複ファイル通知ダイアログ、非対応拡張子エラー表示 |
| チャット UI | Markdown / KaTeX 描画、生 HTML 無効化（XSS 防止）、ストリーミング表示、中断ボタン、権限要求承認カード、差分承認 UI |
| エージェント設定 UI | 自動検出結果表示、「未取得」状態の誠実表示、画像非対応エージェント選択時の警告、API キー切替確認ダイアログ |
| PDF 関連 UI | ページ移動・ズーム・タブ切替、変換エラー箇所の表示、メタデータ表示切替設定 |
| エラー表示 | Rust `ApiError` のコード別ユーザー向けメッセージ表示とリトライ操作 |

### 2.3 Python / Science MCP テスト（`pytest`）

| 対象領域 | 主なテストケース |
|---|---|
| 微分・積分 | 多項式・三角関数・指数対数・高階微分・偏微分・定積分・不定積分の正確性および LaTeX 出力 |
| 方程式・連立・ODE | 代数方程式、連立一次方程式、基本常微分方程式の求解 |
| 行列演算 | 行列式、逆行列、特異行列エラー、行列積、固有値・固有ベクトル |
| 等式検証 | 恒等式 `passed`、不等式 `failed`、条件付き `inconclusive` の判定と根拠メタデータ |
| 単位変換・次元解析 | SI 単位変換（例: `km/h` → `m/s`、`eV` → `J`）、運動方程式・電磁気式の次元一致/不一致検出 |
| グラフ生成 | `plot_function` の SVG/PNG 生成、指定 `artifacts/` 外への出力拒否、ファイル名 UUID 生成確認 |
| **セキュリティ・不正入力** | `__import__('os').system('echo pwn')`、`().__class__.__base__.__subclasses__()`、`open(...)`、属性アクセス、巨大指数（`10**1000000`）、長大文字列の完全拒否 |
| **実行タイムアウト** | 意図的に重い記号積分/展開に対するタイムアウト発動とワーカープロセス終了確認 |

---

## 3. 統合テストシナリオ

Mock Agent Adapter および Mock Google Drive HTTP サーバー（`wiremock` 等）を用いて、以下のエンドツーエンドシナリオを自動検証する。

### 3.1 大学数学フルワークフロー（12 ステップ）

1. 大学数学のサンプル問題 PDF を取り込む（UUID・SHA-256・サムネイル生成確認）。
2. 問題文を認識する（印刷のみモード）。
3. 認識した問題文・数式を編集し保存する。
4. AI（Mock Adapter）で解答を生成する（構造化 JSON 検証・Science MCP 検証連携）。
5. Typst で日本語・数式入り解答 PDF を生成する。
6. 生成した解答について AI チャットで質問し、ストリーミング応答を受信する。
7. チャット経由で解答の一部修正を指示し、差分確認・承認する（Phase 5；MVP では解答エディタ修正）。
8. 過去版を上書きせず新しい解答バージョン（v2）と PDF を保存する。
9. Google Drive（Mock Server）へ問題 PDF と最新解答 PDF をアップロードする（`files.create`）。
10. 解答更新後に再アップロードし、**同一 Drive ファイル ID** で `files.update` が呼ばれることを検証する。
11. アプリのコア状態（DB 接続・ストア）を再初期化する（再起動シミュレーション）。
12. 保存済みの問題・解答バージョン履歴・PDF 参照・会話履歴・Drive マッピングが欠損なく復元されることを検証する。

### 3.2 大学物理特化シナリオ（3 ケース）

1. **図を含む問題**（例: 斜面上の物体運動）: ページ画像切り出し → 問題認識 → 座標系・仮定・自由物体に関する解答生成 → PDF 埋め込み。
2. **単位・次元のある問題**（例: 電磁気学・熱力学）: Science MCP の `convert_units` / `check_dimensions` 実行 → `verification_results` 記録 → PDF 出力。
3. **微分方程式・グラフを含む問題**（例: 減衰振動・RC 回路）: `solve_ode` / `plot_function` 実行 → SVG/PNG アーティファクト保存 → Typst PDF へグラフ埋め込み。

---

## 4. テストフィクスチャ計画（`fixtures/`）

```text
fixtures/
├── math/
│   ├── calculus_integral.pdf        # 積分の印刷問題サンプル
│   ├── linear_algebra_matrix.png    # 行列・固有値の画像問題サンプル
│   └── complex_formulas.json        # LaTeX→Typst 変換回帰テスト用数式集
├── physics/
│   ├── inclined_plane_diagram.pdf   # 図を含む力学問題サンプル
│   ├── electromagnetism_units.png   # 単位・次元を含む電磁気問題サンプル
│   └── damped_oscillation_ode.pdf   # 微分方程式＋グラフ問題サンプル
└── mocks/
    ├── codex_rpc_transcript.jsonl   # Codex App Server モック応答集
    ├── acp_rpc_transcript.jsonl     # ACP モック応答集
    └── gdrive_responses.json        # Google Drive API v3 モック応答集
```

---

## 5. CI / 品質ゲート（GitHub Actions）

すべての PR・マージ前に以下を自動実行する（Phase 1 以降、各コンポーネント追加に合わせて順次有効化）。

| 領域 | 実行コマンド |
|---|---|
| Rust フォーマット | `cargo fmt --all -- --check` |
| Rust 静的解析 | `cargo clippy --all-targets -- -D warnings` |
| Rust テスト | `cargo test` |
| TypeScript 型検査・Lint | `pnpm typecheck && pnpm lint` |
| React テスト | `pnpm test` |
| Python Lint / Format | `uv run ruff check && uv run ruff format --check` |
| Python テスト | `uv run pytest` |

---

## 関連ドキュメント

- [開発ガイド（development.md）](./development.md)
- [実装・検証状況（implementation-status.md）](./implementation-status.md)
- [Science MCP Server 仕様（science-mcp.md）](./science-mcp.md)
