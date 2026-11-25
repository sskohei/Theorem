# ADR-0001: デスクトップアプリ基盤としての Tauri v2 + React / TypeScript + Rust 採用

- **ステータス**: Accepted
- **日付**: 2026-11-25
- **関連フェーズ**: Phase 1 / Phase 6

## コンテキスト（背景・課題）

FormulaDesk は Windows 11 を最優先としつつ将来的に macOS・Linux にも対応する、オープンソース・ローカルファーストのデスクトップアプリである。PDF 閲覧（PDF.js）、数式描画（KaTeX）、3 カラム学習 UI に加え、外部 AI ハーネス子プロセスの非同期制御、SQLite 永続化、OS キーチェーン、セキュリティ隔離を安全かつ軽量に実現する必要がある。

## 決定内容

- デスクトップフレームワークに **Tauri v2** を採用する。
- フロントエンドに **React / TypeScript / Vite / pnpm / Tailwind CSS / shadcn/ui / Zustand** を採用する。
- アプリケーションコアに **Rust / Tokio / sqlx / reqwest / serde** を採用する。
- UI からは SQLite や OS キーチェーンへ直接アクセスさせず、Tauri v2 の最小権限 Capability と厳格 CSP、型安全な IPC コマンド経由で Rust コアと通信する。

## 検討した代替案

1. **Electron + Node.js**: 実績豊富だが、Chromium 同梱による配布サイズ・メモリ消費が大きく、子プロセスやシステム境界の型安全・軽量運用で Tauri + Rust に劣る。
2. **ネイティブ GUI（Swift / WinUI / Qt / Iced）**: PDF.js・KaTeX・Markdown の高品質レンダリングとクロスプラットフォーム UI 開発効率の確保コストが高い。

## 結果・トレードオフ・リスク

- **利点**: 配布サイズ・メモリ効率が高く、Rust の型安全性・非同期制御（Tokio）・ACP クレート（`agent-client-protocol`）や Typst エコシステムとの親和性が高い。
- **トレードオフ**: Rust + TypeScript の 2 言語管理が必要になり、IPC 型の同期保守が求められる。Windows ビルド時に MSVC Build Tools が必要。
