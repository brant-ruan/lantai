// Text that changes with state but keeps the width of its longest variant, so the controls
// next to it never move.
export function LabelSwap<K extends string>({
  current,
  labels,
}: {
  current: K;
  labels: Record<K, string>;
}) {
  return (
    <span className="label-swap">
      {(Object.keys(labels) as K[]).map((k) => (
        <span key={k} aria-hidden={k !== current}>
          {labels[k]}
        </span>
      ))}
    </span>
  );
}
