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
        className="absolute inset-0 bg-black/50 transition-opacity duration-[var(--transition-normal)]"
        onClick={onClose}
      />
      {/* Panel */}
      <div
        className={[
          'relative z-10 w-full max-w-md mx-[var(--space-6)]',
          'bg-[var(--color-bg-elevated)] border border-[var(--color-border)]',
          'rounded-[var(--radius-lg)] shadow-[var(--shadow-lg)]',
          'flex flex-col max-h-[80vh]',
        ].join(' ')}
      >
        {title && (
          <div className="px-[var(--space-6)] pt-[var(--space-5)] pb-[var(--space-3)] border-b border-[var(--color-border-subtle)]">
            <h2 className="text-[var(--text-lg)] font-semibold text-[var(--color-text)]">
              {title}
            </h2>
          </div>
        )}
        <div className="px-[var(--space-6)] py-[var(--space-5)] overflow-y-auto flex-1">
          {children}
        </div>
        {footer && (
          <div className="px-[var(--space-6)] py-[var(--space-4)] border-t border-[var(--color-border-subtle)] flex justify-end gap-[var(--space-3)]">
            {footer}
          </div>
        )}
      </div>
    </div>,
    document.body,
  )
}
