# 実装・技術検証状況（Implementation Status）

> **最終更新**: 2026-11-25
> **現在フェーズ**: 設計ドキュメント整備完了 → **Phase 0（技術検証）着手待ち**
> **誠実性ルール**: 未実装機能を完成済みと記載しない。モックテスト合格と外部サービス実接続確認を明確に区別する。外部認証が必要な項目は認証情報なしで「動作確認済み」にしない。

---

## 1. ドキュメント整備状況

| ドキュメント | 状態 | 備考 |
|---|---|---|
| `docs/product-specification.md` | 作成済み (v1.0 Draft) | 製品全体仕様・MVP 条件 |
| `docs/requirements.md` | 作成済み (v1.0 Draft) | 機能・非機能要件・禁止事項 |
| `docs/architecture.md` | 作成済み (v1.0 Draft) | 全体構成・参考 OSS 調査・ストレージ設計 |
| `docs/database.md` | 作成済み (v1.0 Draft) | 全 21 テーブル・ER 図・インデックス・マイグレーション計画 |
| `docs/agent-integration.md` | 作成済み (v1.0 Draft) | 共通 IF・イベント・Capability・プロセス管理 |
| `docs/provider-compatibility.md` | 作成済み (v1.0 Draft) | 机上調査済み・実機検証待ち |
| `docs/science-mcp.md` | 作成済み (v1.0 Draft) | ツール仕様・AST 安全パーサー設計 |
| `docs/pdf-generation.md` | 作成済み (v1.0 Draft) | Typst・LaTeX 変換・構造化スキーマ v1 |
| `docs/google-drive.md` | 作成済み (v1.0 Draft) | OAuth PKCE・`drive.file`・永続ジョブキュー |
| `docs/security.md` | 作成済み (v1.0 Draft) | 脅威モデル・隔離・Tauri Capability/CSP |
| `docs/ui-design.md` | 作成済み (v1.0 Draft) | 3 カラム画面・6 タブ・11 設定カテゴリ |
| `docs/testing.md` | 作成済み (v1.0 Draft) | Rust/React/Python/統合テスト計画 |
| `docs/development.md` | 作成済み (v1.0 Draft) | 環境構築・規約・フェーズ完了チェックリスト |
| `docs/roadmap.md` | 作成済み (v1.0 Draft) | Phase 0〜6 計画・GitHub Issue 対応 |
| `docs/adr/` (ADR-0001〜0006) | 作成済み | 主要設計判断（一部 Phase 0 検証待ち） |

---

## 2. Phase 0 技術検証（PoC）トラッカー

ステータス凡例: `動作確認済み` / `未検証` / `対応不可` / `利用条件の確認が必要`

### 2.1 AI ハーネス PoC

| # | 検証項目 | ステータス | 代替手段・技術的リスク・メモ |
|---|---|---|---|
| 1 | Codex App Server の起動 | `未検証` | `codex app-server` (JSON-RPC 2.0 / stdio)。Windows 環境でのプロセス終了管理を確認 |
| 2 | Claude Code への接続 | `未検証` | 公式プロセス / Agent SDK / ACP アダプターの選定が必要 |
| 3 | Antigravity ACP への接続 | `未検証` | `agy acp` (JSON-RPC 2.0 / stdio)。`agent-client-protocol` crate 検証 |
| 4 | カスタム ACP エージェントへの接続 | `未検証` | 汎用 ACP クライアント＋Capability 動的判定を確認 |
| 5 | 公式認証の確認 | `未検証` | 外部認証情報が必要（認証なしで成功扱いにしない） |
| 6 | サブスクリプション利用条件の確認 | `利用条件の確認が必要` | Claude Code サブスク OAuth はサードパーティ利用制限あり（API キー代替要確認）。Codex・Antigravity も対話クライアント適合性を再確認 |
| 7 | 画像入力 | `未検証` | 問題 PDF ページ PNG による大学数学・物理数式認識精度を確認 |
| 8 | ストリーミング応答 | `未検証` | チャンク逐次配信と UI 反映レイテンシ確認 |
| 9 | セッション再開 | `未検証` | 非対応時は SQLite 正本からの履歴引き継ぎへフォールバック |
| 10 | 処理のキャンセル | `未検証` | 中断応答および孤児プロセス非残留を Windows で確認 |
| 11 | MCP ツールの登録 | `未検証` | グローバル設定を変更しないセッション単位登録方法を確認 |
| 12 | 権限制御 | `未検証` | 任意シェル禁止・ワークスペース外アクセス遮断の実効性を確認 |

### 2.2 PDF / Typst PoC

| # | 検証項目 | ステータス | 代替手段・技術的リスク・メモ |
|---|---|---|---|
| 1 | Typst による PDF 生成 | `未検証` | Rust `typst` crate 組み込み vs Typst CLI サイドカーを比較確定 |
| 2 | 日本語の組版 | `未検証` | 再配布可能フォント（OFL 等）の選定と禁則・改行品質確認 |
| 3 | 大学数学の複雑な数式 | `未検証` | 行列・偏微分・多重積分・証明・長い数式の改行を確認 |
| 4 | LaTeX から Typst への変換 | `未検証` | `mitex` / `tex2typst` / 制限 AST 変換を比較、未対応記法エラー設計確認 |
| 5 | 図・グラフの埋め込み | `未検証` | Matplotlib 出力 SVG / 300 DPI PNG の埋め込み品質確認 |
| 6 | PDF.js による PDF 表示 | `未検証` | Tauri WebView 内でのテキスト選択・ズーム・サムネイル・ページ画像化確認 |

### 2.3 Google Drive PoC

| # | 検証項目 | ステータス | 代替手段・技術的リスク・メモ |
|---|---|---|---|
| 1 | Google OAuth 認証（PKCE + ループバック） | `未検証` | 実 Google アカウントと `drive.file` スコープでの検証が必要 |
| 2 | フォルダ作成 | `未検証` | `My Drive/FormulaDesk/...` 階層作成・既存参照確認 |
| 3 | ファイルのアップロード（`files.create`） | `未検証` | `appProperties` 冪等キー付与を含む |
| 4 | 同一ファイル ID での更新（`files.update`） | `未検証` | 共有リンク維持・`md5Checksum`/`modifiedTime` による競合検出確認 |

---

## 3. フェーズ別実装ステータス

| Phase | Issue | コンポーネント | 実装状態 | 自動テスト (Mock/Unit) | 実サービス接続検証 |
|---|---|---|---|---|---|
| Phase 0 | [#1](https://github.com/sskohei/Theorem/issues/1) | 技術検証 PoC | 未着手 | 未実行 | 未実施 |
| Phase 1 | [#2](https://github.com/sskohei/Theorem/issues/2) | Tauri / React 3 カラム / SQLite / ファイル・PDF 基盤 | 未着手 | 未実行 | N/A |
| Phase 2 | [#3](https://github.com/sskohei/Theorem/issues/3) | AI アダプター / 問題認識 / 解答生成 / チャット / PDF / バージョン管理 | 未着手 | 未実行 | 未実施 |
| Phase 3 | [#4](https://github.com/sskohei/Theorem/issues/4) | Science MCP Server / 計算・検証・グラフ / PDF 埋め込み | 未着手 | 未実行 | N/A (ローカル) |
| Phase 4 | [#5](https://github.com/sskohei/Theorem/issues/5) | Google Drive OAuth / 永続ジョブキュー / 更新・競合検出 | 未着手 | 未実行 | 未実施 |
| Phase 5 | [#6](https://github.com/sskohei/Theorem/issues/6) | 部分修正・差分承認 / 答案添削 / 全文検索 / 高度物理図 | 未着手 | 未実行 | 未実施 |
| Phase 6 | [#7](https://github.com/sskohei/Theorem/issues/7) | インストーラー / クロスプラットフォーム / CI・配布 / 日英対応 | 未着手 | 未実行 | N/A |

---

## 4. 未解決の課題・確認待ち事項

1. **AI サブスクリプション利用規約の最終確認**: Codex・Claude Code・Antigravity それぞれについて、ローカル学習クライアントから利用可能な認証方式を Phase 0 で確定する。
2. **Typst 統合方式の確定**: Rust クレート組み込み vs CLI サイドカーを Phase 0 PoC で比較し [ADR-0004](./adr/0004-typst-pdf-generation-and-latex-conversion.md) を確定する。
3. **LaTeX → Typst 変換ライブラリの選定**: 大学数学・物理の数式カバレッジとエラー検出の厳密性を検証する。
4. **配布戦略（Phase 6）**: Windows エンドユーザー向けに Python (`uv`) / Science MCP 環境・Typst・日本語フォントをどうバンドルまたはガイドするかを検討する。

---

## 関連ドキュメント

- [開発ロードマップ（roadmap.md）](./roadmap.md)
- [プロバイダー互換性と認証条件（provider-compatibility.md）](./provider-compatibility.md)
- [ADR 一覧（adr/README.md）](./adr/README.md)
