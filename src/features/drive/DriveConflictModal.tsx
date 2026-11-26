import { AlertTriangle, CopyPlus, RefreshCw, Trash2, UploadCloud, X } from "lucide-react";
import { Badge } from "@/components/ui/Badge";
import { Button } from "@/components/ui/Button";
import { useDriveStore } from "@/stores/useDriveStore";

export function DriveConflictModal() {
  const { activeConflictJob, closeConflictModal, resolveConflict } = useDriveStore();

  if (!activeConflictJob) return null;

  const isRemoteDeleted = activeConflictJob.lastErrorCode === "REMOTE_DELETED";

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Google Drive 競合の確認"
      className="fixed inset-0 z-50 flex items-center justify-center bg-slate-950/55 p-4 backdrop-blur-xs"
    >
      <div className="w-full max-w-lg rounded-xl border border-slate-200 bg-white shadow-2xl dark:border-slate-700 dark:bg-slate-900">
        <div className="flex items-center justify-between border-b border-slate-200 px-4 py-3 dark:border-slate-800">
          <div className="flex items-center gap-2 text-sm font-semibold text-amber-900 dark:text-amber-200">
            {isRemoteDeleted ? (
              <Trash2 className="h-4 w-4 text-red-600 dark:text-red-400" />
            ) : (
              <AlertTriangle className="h-4 w-4 text-amber-600 dark:text-amber-400" />
            )}
            <span>
              {isRemoteDeleted
                ? "Google Drive 側でファイルが削除されています"
                : "Google Drive 側で外部変更を検出しました（無断上書きを防止）"}
            </span>
          </div>
          <Button
            size="icon"
            variant="ghost"
            aria-label="閉じる"
            onClick={closeConflictModal}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>

        <div className="space-y-3 p-4 text-xs text-slate-700 dark:text-slate-300">
          <div className="rounded-lg border border-amber-200 bg-amber-50/70 p-3 dark:border-amber-900 dark:bg-amber-950/40">
            <div className="flex flex-wrap items-center gap-2 font-semibold text-slate-900 dark:text-slate-100">
              <span>対象ファイル: {activeConflictJob.targetFilename}</span>
              <Badge variant="outline">{activeConflictJob.logicalRole}</Badge>
            </div>
            {activeConflictJob.lastErrorMessage && (
              <p className="mt-1.5 leading-relaxed text-amber-900 dark:text-amber-200">
                {activeConflictJob.lastErrorMessage}
              </p>
            )}
          </div>

          {!isRemoteDeleted && (
            <dl className="grid grid-cols-[120px_1fr] gap-y-1 rounded-md border border-slate-200 bg-slate-50 p-2.5 text-[11px] dark:border-slate-800 dark:bg-slate-950/60">
              <dt className="text-slate-500">Drive ファイル ID</dt>
              <dd className="font-mono break-all">{activeConflictJob.driveFileId ?? "—"}</dd>
              {activeConflictJob.conflictRemoteMd5 && (
                <>
                  <dt className="text-slate-500">Drive 側 MD5</dt>
                  <dd className="font-mono break-all">{activeConflictJob.conflictRemoteMd5}</dd>
                </>
              )}
              {activeConflictJob.conflictRemoteModifiedTime && (
                <>
                  <dt className="text-slate-500">Drive 更新日時</dt>
                  <dd>{activeConflictJob.conflictRemoteModifiedTime}</dd>
                </>
              )}
              {activeConflictJob.conflictRemoteVersion && (
                <>
                  <dt className="text-slate-500">Drive バージョン</dt>
                  <dd>v{activeConflictJob.conflictRemoteVersion}</dd>
                </>
              )}
            </dl>
          )}

          <p className="text-[11px] leading-relaxed text-slate-500 dark:text-slate-400">
            {isRemoteDeleted
              ? "ローカルの正本ファイルは保持されています。新しいファイルとして Google Drive へ再作成するか、キャンセルしてください。"
              : "「ローカルで上書き」を選ぶと同一 Drive ファイル ID（共有リンク）を維持したままローカル版で更新します。「別ファイルとして保存」を選ぶと Drive 側の変更を残して別名ファイルを作成します。"}
          </p>
        </div>

        <div className="flex flex-wrap items-center justify-end gap-2 border-t border-slate-200 px-4 py-3 dark:border-slate-800">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => void resolveConflict(activeConflictJob.id, "cancel")}
          >
            キャンセル
          </Button>
          {isRemoteDeleted ? (
            <Button
              size="sm"
              variant="primary"
              onClick={() =>
                void resolveConflict(activeConflictJob.id, "recreate_after_remote_delete")
              }
            >
              <UploadCloud className="h-3.5 w-3.5" />
              <span>新規ファイルとして再作成</span>
            </Button>
          ) : (
            <>
              <Button
                size="sm"
                variant="outline"
                onClick={() => void resolveConflict(activeConflictJob.id, "save_as_new")}
              >
                <CopyPlus className="h-3.5 w-3.5" />
                <span>別ファイルとして保存</span>
              </Button>
              <Button
                size="sm"
                variant="primary"
                onClick={() => void resolveConflict(activeConflictJob.id, "overwrite")}
              >
                <RefreshCw className="h-3.5 w-3.5" />
                <span>ローカルで上書き</span>
              </Button>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
