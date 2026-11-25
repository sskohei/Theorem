# Architecture Decision Records (ADR)

本ディレクトリには、FormulaDesk における重要なアーキテクチャおよび技術選定の意思決定記録（ADR）を保管する。

---

## ADR 一覧

| 番号 | タイトル | ステータス | 更新日 |
|---|---|---|---|
| [0001](./0001-desktop-stack-tauri-react-rust.md) | デスクトップアプリ基盤としての Tauri v2 + React / TypeScript + Rust 採用 | Accepted | 2026-11-25 |
| [0002](./0002-external-ai-harness-and-adapter-layer.md) | 外部公式 AI ハーネスの子プロセス統合と共通アダプター層（ハイブリッドプロトコル） | Accepted | 2026-11-25 |
| [0003](./0003-local-first-sqlite-and-filesystem-storage.md) | ローカルファースト設計：SQLite メタデータ管理とファイルシステム分離 | Accepted | 2026-11-25 |
| [0004](./0004-typst-pdf-generation-and-latex-conversion.md) | Typst による PDF 生成と LaTeX→Typst 変換層（実行方式は Phase 0 で確定） | Proposed | 2026-11-25 |
| [0005](./0005-science-mcp-server-python-uv.md) | Science MCP Server（Python / uv / AST 安全パーサー）の採用 | Accepted | 2026-11-25 |
| [0006](./0006-google-drive-export-oauth-drive-file.md) | Google Drive のエクスポート用途限定と OAuth 2.0 PKCE + `drive.file` スコープ採用 | Accepted | 2026-11-25 |

---

## ADR のステータス定義

- **Proposed**: 提案中・Phase 0 PoC 等の検証結果待ち
- **Accepted**: 採用確定
- **Deprecated**: 非推奨
- **Superseded**: 後続の ADR によって置き換え済み

---

## 新規 ADR テンプレート

```markdown
# ADR-XXXX: <決定タイトル>

- **ステータス**: Proposed | Accepted | Deprecated | Superseded
- **日付**: YYYY-MM-DD
- **関連フェーズ**: Phase X

## コンテキスト（背景・課題）

## 決定内容

## 検討した代替案

## 結果・トレードオフ・リスク
```
