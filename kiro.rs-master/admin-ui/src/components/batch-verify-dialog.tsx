import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'
import { Badge } from '@/components/ui/badge'
import { Trash2 } from 'lucide-react'

export interface VerifyResult {
  id: number
  status: 'pending' | 'verifying' | 'success' | 'failed'
  usage?: string
  error?: string
  email?: string
}

interface BatchVerifyDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  verifying: boolean
  progress: { current: number; total: number }
  results: Map<number, VerifyResult>
  onCancel: () => void
  /** Deletesinglefailed credentials */
  onDelete?: (id: number) => void
  /** one clickDeleteAllFailedCredential */
  onDeleteFailed?: () => void
  /** Deleteproceedin (Disablebutton) */
  deleting?: boolean
}

export function BatchVerifyDialog({
  open,
  onOpenChange,
  verifying,
  progress,
  results,
  onCancel,
  onDelete,
  onDeleteFailed,
  deleting,
}: BatchVerifyDialogProps) {
  const resultsArray = Array.from(results.values())
  const successCount = resultsArray.filter(r => r.status === 'success').length
  const failedCount = resultsArray.filter(r => r.status === 'failed').length

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>Bulk validate</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-4">
          {/* progressShow */}
          {verifying && (
            <div className="space-y-2">
              <div className="flex justify-between text-sm">
                <span>Validation progress</span>
                <span>{progress.current} / {progress.total}</span>
              </div>
              <div className="w-full bg-secondary rounded-full h-2">
                <div
                  className="bg-primary h-2 rounded-full transition-all"
                  style={{ width: `${(progress.current / progress.total) * 100}%` }}
                />
              </div>
            </div>
          )}

          {/* statistics info */}
          {results.size > 0 && (
            <div className="flex justify-between items-center text-sm font-medium">
              <span>Validation result</span>
              <span>
                Success: {successCount} / Failed: {failedCount}
              </span>
            </div>
          )}

          {/* one clickDelete failedCredential */}
          {!verifying && failedCount > 0 && onDeleteFailed && (
            <Button
              type="button"
              variant="destructive"
              size="sm"
              className="w-full"
              disabled={deleting}
              onClick={onDeleteFailed}
            >
              <Trash2 className="h-3.5 w-3.5" />
              {deleting ? 'Deleting…' : `Delete all failed (${failedCount})`}
            </Button>
          )}

          {/* resultList */}
          {results.size > 0 && (
            <div className="max-h-[400px] overflow-y-auto border rounded-md p-2 space-y-1">
              {resultsArray.map((result) => (
                <div
                  key={result.id}
                  className={`text-sm p-2 rounded ${
                    result.status === 'success'
                      ? 'bg-green-50 text-green-700 dark:bg-green-950 dark:text-green-300'
                      : result.status === 'failed'
                      ? 'bg-red-50 text-red-700 dark:bg-red-950 dark:text-red-300'
                      : result.status === 'verifying'
                      ? 'bg-blue-50 text-blue-700 dark:bg-blue-950 dark:text-blue-300'
                      : 'bg-gray-50 text-gray-700 dark:bg-gray-950 dark:text-gray-300'
                  }`}
                >
                  <div className="flex items-start justify-between gap-2">
                    <div className="flex items-center gap-2 min-w-0">
                      <span className="font-medium shrink-0">Credential #{result.id}</span>
                      {result.email && (
                        <span className="text-xs opacity-80 truncate">{result.email}</span>
                      )}
                      {result.status === 'success' && result.usage && (
                        <Badge variant="secondary" className="text-xs shrink-0">
                          {result.usage}
                        </Badge>
                      )}
                    </div>
                    <div className="flex items-center gap-2 shrink-0">
                      {!verifying && result.status === 'failed' && onDelete && (
                        <button
                          type="button"
                          title="Delete this failed credential"
                          disabled={deleting}
                          onClick={() => onDelete(result.id)}
                          className="text-red-500 hover:text-red-700 disabled:opacity-40"
                        >
                          <Trash2 className="h-3.5 w-3.5" />
                        </button>
                      )}
                      <span>
                        {result.status === 'success' && '✓'}
                        {result.status === 'failed' && '✗'}
                        {result.status === 'verifying' && '⏳'}
                        {result.status === 'pending' && '⋯'}
                      </span>
                    </div>
                  </div>
                  {result.error && (
                    <div className="text-xs mt-1 opacity-90">
                      Error: {result.error}
                    </div>
                  )}
                </div>
              ))}
            </div>
          )}

          {/* hint info */}
          {verifying && (
            <p className="text-xs text-muted-foreground">
              💡 Validation runs concurrently in the background. You can close this window and validation will continue. When done you can remove the invalid ones in this window/banned account.
            </p>
          )}
        </div>

        <div className="flex justify-end gap-2">
          {verifying ? (
            <>
              <Button
                type="button"
                variant="outline"
                onClick={() => onOpenChange(false)}
              >
                Run in background
              </Button>
              <Button
                type="button"
                variant="destructive"
                onClick={onCancel}
              >
                Cancel validation
              </Button>
            </>
          ) : (
            <Button
              type="button"
              onClick={() => onOpenChange(false)}
            >
              Close
            </Button>
          )}
        </div>
      </DialogContent>
    </Dialog>
  )
}
