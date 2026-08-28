/**
 * The console's handful of shared pieces.
 *
 * Small on purpose. The app has a real component library because it has forty
 * screens of bespoke layout; the console has six pages that are mostly tables,
 * and the useful abstraction there is a page header, a section header, a card
 * and a status pill. Anything more would be a framework standing between the
 * generated tokens and six files.
 *
 * Note what is *not* here: icons. See the note at the top of app.css — a back
 * office is read, not browsed, and a glyph beside a word in a table is a
 * second thing to decode. Status is a pill with the status written in it.
 */

import type { ReactNode } from 'react';

/** Title, one line of what the page is for, and the page-level actions. */
export function PageHead({
  title,
  lede,
  actions,
}: {
  title: string;
  lede?: ReactNode;
  actions?: ReactNode;
}) {
  return (
    <header className="pagehead">
      <div>
        <h1>{title}</h1>
        {lede ? <p className="lede">{lede}</p> : null}
      </div>
      {actions ? <div className="actions">{actions}</div> : null}
    </header>
  );
}

/** A heading over a block, with an optional count and a right-hand note. */
export function SectionHead({
  title,
  count,
  note,
  children,
}: {
  title: string;
  count?: number;
  /** Right-aligned context — "A – Z", "updated live". */
  note?: ReactNode;
  /** Right-aligned controls. */
  children?: ReactNode;
}) {
  return (
    <div className="sectionhead">
      <h2>{title}</h2>
      {count != null ? <span className="count">{count}</span> : null}
      <span className="spacer" />
      {note ? <span className="note">{note}</span> : null}
      {children}
    </div>
  );
}

export function Card({
  children,
  flush,
  className,
}: {
  children: ReactNode;
  /** For a card whose only child is a table — the table draws its own padding. */
  flush?: boolean;
  className?: string;
}) {
  return (
    <div className={['card', flush ? 'flush' : '', className ?? ''].filter(Boolean).join(' ')}>
      {children}
    </div>
  );
}

/** A table inside a flush card, scrolling horizontally rather than the page. */
export function TableCard({ children }: { children: ReactNode }) {
  return (
    <Card flush>
      <div className="tablewrap">
        <table>{children}</table>
      </div>
    </Card>
  );
}

/**
 * A number with a label and a line of context.
 *
 * `to` makes the whole tile a link. Only pass it where the number has
 * somewhere to go: a count with nowhere to go is decoration, and a tile that
 * looks clickable and is not is worse than a plain one.
 */
export function Stat({
  label,
  value,
  hint,
  alert,
}: {
  label: string;
  value: string;
  hint?: string;
  alert?: boolean;
}) {
  return (
    <div className={alert ? 'stat alert' : 'stat'}>
      <span className="k">{label}</span>
      <span className="v">{value}</span>
      {hint ? <span className="h">{hint}</span> : null}
    </div>
  );
}

/** Pick exactly one. The trough shape says "these are alternatives". */
export function Segmented<T extends string>({
  options,
  value,
  onChange,
  label,
}: {
  options: { key: T; label: string; count?: number }[];
  value: T;
  onChange: (key: T) => void;
  label: string;
}) {
  return (
    <div className="segmented" role="group" aria-label={label}>
      {options.map((option) => (
        <button
          key={option.key}
          type="button"
          aria-pressed={option.key === value}
          onClick={() => onChange(option.key)}
        >
          {option.label}
          {option.count != null ? <span className="tally">{option.count}</span> : null}
        </button>
      ))}
    </div>
  );
}

/**
 * A switch, not a button labelled with its own state.
 *
 * The old settings page had a button reading "On", which is ambiguous in the
 * way that matters: you cannot tell by looking whether it reports the state or
 * performs the act. A switch reports; pressing it changes what it reports.
 */
export function Switch({
  on,
  onChange,
  label,
  disabled,
}: {
  on: boolean;
  onChange: (next: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      className="switch"
      role="switch"
      aria-checked={on}
      aria-pressed={on}
      aria-label={label}
      disabled={disabled}
      onClick={() => onChange(!on)}
    >
      <span className="knob" />
    </button>
  );
}

/**
 * An empty state is an instruction, not an apology. `title` says what is not
 * there; the body says what to do about it.
 */
export function Empty({ title, children }: { title: string; children?: ReactNode }) {
  return (
    <div className="empty">
      <strong>{title}</strong>
      {children}
    </div>
  );
}

/** A status pill. The word is the content; the colour only reinforces it. */
export function Chip({
  tone = 'quiet',
  children,
}: {
  tone?: 'due' | 'paid' | 'late' | 'void' | 'ink' | 'quiet' | 'live';
  children: ReactNode;
}) {
  return <span className={`chip chip-${tone}`}>{children}</span>;
}
