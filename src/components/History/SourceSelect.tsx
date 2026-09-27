/**
 * A compact picker for the device, interface or GPU a history panel shows.
 *
 * The selection lives in the panel's state for the session only: source
 * identifiers can be derived from hardware (a MAC address, a drive serial),
 * so they are deliberately **not** written to the persisted visualization
 * preferences. On relaunch the panel picks its sensible default again.
 */
export function SourceSelect({
  label,
  options,
  value,
  onChange,
}: {
  readonly label: string;
  readonly options: readonly { readonly value: string; readonly label: string }[];
  readonly value: string;
  readonly onChange: (value: string) => void;
}) {
  if (options.length <= 1) return null;
  return (
    <select
      className="history-panel__select"
      aria-label={label}
      value={value}
      onChange={(event) => onChange(event.target.value)}
    >
      {options.map((option) => (
        <option key={option.value} value={option.value}>
          {option.label}
        </option>
      ))}
    </select>
  );
}
