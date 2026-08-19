/**
 * Legacy Dialog wrapper — bridges the old imperative open/onClose API
 * to the new Radix-based Dialog. Will be removed once all pages migrate
 * to the compound Dialog pattern in Stage 2.
 */
import { type ReactNode } from 'react'
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from './Dialog'

interface LegacyDialogProps {
  open: boolean
  onClose: () => void
  title?: string
  children: ReactNode
  footer?: ReactNode
}

export function LegacyDialog({ open, onClose, title, children, footer }: LegacyDialogProps) {
  return (
    <Dialog open={open} onOpenChange={(v) => { if (!v) onClose() }}>
      <DialogContent hideClose>
        {title && (
          <DialogHeader>
            <DialogTitle>{title}</DialogTitle>
          </DialogHeader>
        )}
        <div className="overflow-y-auto max-h-[60vh]">{children}</div>
        {footer && <DialogFooter>{footer}</DialogFooter>}
      </DialogContent>
    </Dialog>
  )
}
