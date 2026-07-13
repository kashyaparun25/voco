import "./ui.css";

interface ToggleProps {
  checked: boolean;
  onChange: () => void;
  disabled?: boolean;
  "aria-label"?: string;
}

/** Theme-native switch with a clearly visible off-track. */
export function Toggle({ checked, onChange, disabled, ...rest }: ToggleProps) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      disabled={disabled}
      onClick={onChange}
      className="voco-toggle"
      {...rest}
    >
      <span className="voco-toggle__knob" />
    </button>
  );
}

export default Toggle;
