import { AlertTriangle, CheckCircle2, Info, X, XCircle } from 'lucide-react';
import {
  createContext,
  type ReactNode,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { createPortal } from 'react-dom';
import { Icon } from '../icons/Icon.tsx';
import { cx } from './cx.ts';

export type ToastTone = 'info' | 'success' | 'warning' | 'danger';

export interface ToastOptions {
  title: ReactNode;
  description?: ReactNode;
  tone?: ToastTone;
  /** ms before auto-dismiss; `0` keeps it until dismissed. Default 5000 (danger: 0). */
  duration?: number;
  /** One action button, e.g. Undo. */
  action?: { label: string; onClick: () => void };
  /** Replace an existing toast with the same id instead of stacking ("Saving…" → "Saved"). */
  id?: string;
}

interface ToastRecord extends Omit<ToastOptions, 'tone' | 'duration' | 'id'> {
  id: string;
  tone: ToastTone;
  duration: number;
}

interface ToastApi {
  toast: (options: ToastOptions) => string;
  dismiss: (id: string) => void;
}

const ToastContext = createContext<ToastApi | null>(null);

const TONE_ICON = { info: Info, success: CheckCircle2, warning: AlertTriangle, danger: XCircle } as const;

let counter = 0;

/** Hosts toasts (bottom-right, above the status bar) and announces them via live regions. */
export function ToastProvider({
  children,
  label = 'Notifications',
}: {
  children?: ReactNode;
  label?: string;
}) {
  const [toasts, setToasts] = useState<ToastRecord[]>([]);

  const dismiss = useCallback((id: string) => setToasts((t) => t.filter((x) => x.id !== id)), []);
  const toast = useCallback((o: ToastOptions) => {
    const id = o.id ?? `toast-${++counter}`;
    const tone = o.tone ?? 'info';
    const record: ToastRecord = { ...o, id, tone, duration: o.duration ?? (tone === 'danger' ? 0 : 5000) };
    setToasts((t) => [...t.filter((x) => x.id !== id), record].slice(-4));
    return id;
  }, []);
  const api = useMemo(() => ({ toast, dismiss }), [toast, dismiss]);

  const region = (
    <section className="of-toast-region" aria-label={label}>
      <ol className="of-toast-list">
        {toasts.map((t) => (
          <ToastView key={t.id} toast={t} onDismiss={() => dismiss(t.id)} />
        ))}
      </ol>
    </section>
  );

  return (
    <ToastContext.Provider value={api}>
      {children}
      {typeof document === 'undefined' ? null : createPortal(region, document.body)}
    </ToastContext.Provider>
  );
}

function ToastView({ toast, onDismiss }: { toast: ToastRecord; onDismiss: () => void }) {
  const [paused, setPaused] = useState(false);
  const remaining = useRef(toast.duration);
  const startedAt = useRef(0);

  useEffect(() => {
    if (toast.duration === 0 || paused) return;
    startedAt.current = Date.now();
    const t = setTimeout(onDismiss, remaining.current);
    return () => {
      clearTimeout(t);
      remaining.current -= Date.now() - startedAt.current;
    };
  }, [paused, toast.duration, onDismiss]);

  const urgent = toast.tone === 'danger' || toast.tone === 'warning';
  return (
    <li
      className={cx('of-toast', `of-toast--${toast.tone}`)}
      role={urgent ? 'alert' : 'status'}
      aria-live={urgent ? 'assertive' : 'polite'}
      aria-atomic="true"
      onMouseEnter={() => setPaused(true)}
      onMouseLeave={() => setPaused(false)}
      onFocus={() => setPaused(true)}
      onBlur={() => setPaused(false)}
      onKeyDown={(e) => {
        if (e.key === 'Escape') {
          e.stopPropagation();
          onDismiss();
        }
      }}
    >
      <span className="of-toast__icon" aria-hidden="true">
        <Icon icon={TONE_ICON[toast.tone]} size={18} />
      </span>
      <div className="of-toast__body">
        <div className="of-toast__title">{toast.title}</div>
        {toast.description ? <div className="of-toast__description">{toast.description}</div> : null}
      </div>
      {toast.action ? (
        <button
          type="button"
          className="of-btn of-btn--ghost of-btn--sm of-toast__action"
          onClick={() => {
            toast.action?.onClick();
            onDismiss();
          }}
        >
          <span className="of-btn__label">{toast.action.label}</span>
        </button>
      ) : null}
      <button
        type="button"
        className="of-btn of-btn--ghost of-btn--sm of-btn--icon-only of-toast__close"
        aria-label="Dismiss notification"
        onClick={onDismiss}
      >
        <Icon icon={X} size={14} />
      </button>
    </li>
  );
}

/** `const { toast } = useToast(); toast({ title: 'Saved', tone: 'success' })`. Needs a ToastProvider. */
export function useToast(): ToastApi {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error('useToast must be used inside <ToastProvider>');
  return ctx;
}
