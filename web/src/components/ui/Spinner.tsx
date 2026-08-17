type SpinnerSize = 'sm' | 'md'

interface SpinnerProps {
  size?: SpinnerSize
}

const sizeMap: Record<SpinnerSize, string> = {
  sm: 'w-3.5 h-3.5',
  md: 'w-5 h-5',
}

export function Spinner({ size = 'md' }: SpinnerProps) {
  return (
    <svg
      className={`animate-spin ${sizeMap[size]}`}
      viewBox="0 0 24 24"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
    >
      <circle
        className="opacity-25"
        cx="12"
        cy="12"
        r="10"
        stroke="currentColor"
        strokeWidth="3"
      />
      <path
        className="opacity-75"
        fill="currentColor"
        d="M4 12a8 8 0 018-8V0C5.373 0 0 5.373 0 12h4z"
      />
    </svg>
  )
}
