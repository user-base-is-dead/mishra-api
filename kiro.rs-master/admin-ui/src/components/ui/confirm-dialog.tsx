import * as React from 'react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
} from '@/components/ui/dialog'
import { Button } from '@/components/ui/button'

/** singletimesConfirmofConfigitem */
export interface ConfirmOptions {
  title?: string
  description: React.ReactNode
  /** Confirmbutton text,Default[Confirm] */
  confirmText?: string
  /** Cancelbutton text,Default[Cancel] */
  cancelText?: string
  /** Confirmwhether the buttonUsedangerous(red)style,DeletekindActionsshould setas true */
  destructive?: boolean
}

type ConfirmFn = (options: ConfirmOptions) => Promise<boolean>

const ConfirmContext = React.createContext<ConfirmFn | null>(null)

/** imperative styleConfirm:Back Promise<boolean>,with the native confirm() consistent control flow. */
export function useConfirm(): ConfirmFn {
  const ctx = React.useContext(ConfirmContext)
  if (!ctx) {
    throw new Error('useConfirm must be within <ConfirmProvider> used within')
  }
  return ctx
}

interface PendingState {
  options: ConfirmOptions
  resolve: (value: boolean) => void
}

/** GlobalConfirmdialog Provider:pendingin App root,used by any component in the subtree useConfirm() invoke. */
export function ConfirmProvider({ children }: { children: React.ReactNode }) {
  const [pending, setPending] = React.useState<PendingState | null>(null)

  const confirm = React.useCallback<ConfirmFn>((options) => {
    return new Promise<boolean>((resolve) => {
      setPending({ options, resolve })
    })
  }, [])

  // Closesettle the result at that time:Confirm → true,the rest(Cancel / overlay click / Esc)→ false.
  const settle = React.useCallback(
    (value: boolean) => {
      setPending((prev) => {
        prev?.resolve(value)
        return null
      })
    },
    []
  )

  const opts = pending?.options
  const destructive = opts?.destructive ?? false

  return (
    <ConfirmContext.Provider value={confirm}>
      {children}
      <Dialog
        open={pending !== null}
        onOpenChange={(open) => {
          if (!open) settle(false)
        }}
      >
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{opts?.title ?? 'Please confirm'}</DialogTitle>
            <DialogDescription>{opts?.description}</DialogDescription>
          </DialogHeader>
          <DialogFooter>
            <Button variant="outline" onClick={() => settle(false)}>
              {opts?.cancelText ?? 'Cancel'}
            </Button>
            <Button
              variant={destructive ? 'destructive' : 'default'}
              onClick={() => settle(true)}
            >
              {opts?.confirmText ?? 'Confirm'}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </ConfirmContext.Provider>
  )
}

