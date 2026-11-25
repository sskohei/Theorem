# ADR-0006: Google Drive のエクスポート用途限定と OAuth 2.0 PKCE + `drive.file` スコープ採用

- **ステータス**: Accepted
- **日付**: 2026-11-25
- **関連フェーズ**: Phase 0 / Phase 4

## コンテキスト（背景・課題）

ユーザーはローカルで作成した問題・解答 PDF をクラウドへ保管・共有したいニーズがあるが、端末間双方向同期や SQLite DB の同期は競合破損リスクと開発複雑性が極めて高い。また Google Drive 全体へのアクセス権限（`drive` スコープ）は Restricted Scope に該当し、高額・長期の第三者セキュリティ監査（CASA）が OSS デスクトップアプリ配布の障壁になる。

## 決定内容

- Google Drive は **問題ファイルおよび解答 PDF のエクスポート・更新先** に限定し、ローカルを常に正本とする（SQLite・会話履歴・認証情報はアップロードしない）。
- OAuth 2.0 は **Desktop App Client + Authorization Code Flow + PKCE (S256) + システムブラウザ + ローカルループバック** を採用し、トークンは OS キーチェーンへ保存する。
- スコープは原則 **`https://www.googleapis.com/auth/drive.file`（Non-sensitive scope）のみ** とする。
- 論理対象（`document_id + logical_role`）に対して Google Drive ファイル ID を紐付け、解答更新時は `files.update` で同一 ID を維持（共有リンク保持）する。
- SQLite 永続ジョブキュー・指数バックオフ・`appProperties` 冪等キー・外部変更検出（`md5Checksum` / `modifiedTime`）により信頼性を確保する。

## 検討した代替案

1. **`https://www.googleapis.com/auth/drive`（フルアクセス）**: 任意の既存フォルダを自由に探索できるが、最小権限原則に反し Restricted Scope 審査が必要となり OSS 配布が困難。
2. **SQLite ファイルを含むクラウド同期**: 複数端末での同時編集による DB 破損リスクが高く、プロジェクト基本方針（同期なし）に反する。

## 結果・トレードオフ・リスク

- **利点**: 最小権限で安全かつ OSS 配布時の Google OAuth 審査障壁を最小化でき、共有リンクを壊さず解答 PDF を更新できる。
- **トレードオフ**: `drive.file` の制約により、アプリ作成外の既存フォルダを保存先に選ぶ場合は Google 公式の明示選択フロー等が必要になる。
