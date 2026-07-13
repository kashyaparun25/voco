import type { InputHTMLAttributes } from "react";
import "./ui.css";

interface TextFieldProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "onChange" | "style"> {
  label?: string;
  value: string;
  onChange?: (val: string) => void;
  isDisabled?: boolean;
  /** Applied to the wrapper (matches astryx TextInput usage). */
  style?: React.CSSProperties;
}

/**
 * Theme-native text field. API mirrors the astryx TextInput
 * (label / value / onChange(value) / placeholder / isDisabled) for drop-in use.
 * Exported as `TextInput` so call sites only need their import path changed.
 */
export function TextInput({ label, value, onChange, isDisabled, disabled, className, style, ...rest }: TextFieldProps) {
  return (
    <div className="voco-field" style={style}>
      {label ? <label className="voco-field__label">{label}</label> : null}
      <input
        className={["voco-input", className || ""].filter(Boolean).join(" ")}
        value={value}
        disabled={isDisabled || disabled}
        onChange={(e) => onChange?.(e.target.value)}
        {...rest}
      />
    </div>
  );
}

export default TextInput;
