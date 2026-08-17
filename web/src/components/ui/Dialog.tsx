import { type ReactNode, useEffect, useCallback } from 'react'
import { createPortal } from 'react-dom'

interface DialogProps {
  open: boolean
  onClose: () => void
  title?: string
  children: ReactNode
  footer?: ReactNode
}

export function Dialog({ open, onClose, title, children, footer }: DialogProps) {
  const handleKeyDown = useCallback(
    (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose()
    },
    [onClose],
  )

  useEffect(() => {
    if (open) {
      document.addEventListener('keydown', handleKeyDown)
      document.body.style.overflow = 'hidden'
    }
    return () => {
      document.removeEventListener('keydown', handleKeyDown)
      document.body.style.overflow = ''
    }
  }, [open, handleKeyDown])

  if (!open) return null

  return createPortal(
    <div
      className="fixed inset-0 z-50 flex items-center justify-center"
      role="dialog"
      aria-modal="true"
    >
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-foreground/50 transition-opacity duration-200"
        onClick={onClose}
      />
      {/* Panel */}
      <div
        className={[
          'relative z-10 w-full max-w-md mx-6',
          'bg-card border border-border',
          'rounded-xl shadow-lg',
          'flex flex-col max-h-[80vh]',
        ].join(' ')}
      >
        {title && (
          <div className="px-6 pt-5 pb-3 border-b border-border">
            <h2 className="text-base font-semibold text-card-foreground">
              {title}
            </h2>
          </div>
        )}
        <div className="px-6 py-5 overflow-y-auto flex-1">
          {children}
        </div>
        {footer && (
          <div className="px-6 py-4 border-t border-border flex justify-end gap-3">
            {footer}
          </div>
        )}
      </div>
    </div>,
    document.body,
  )
}
