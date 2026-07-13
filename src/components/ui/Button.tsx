import type { ButtonHTMLAttributes, ReactNode } from "react";
import "./ui.css";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";

interface ButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "type"> {
  label?: string;
  children?: ReactNode;
  variant?: ButtonVariant;
  size?: ButtonSize;
  icon?: ReactNode;
  iconRight?: ReactNode;
  isDisabled?: boolean;
  isLoading?: boolean;
  fullWidth?: boolean;
  type?: "button" | "submit" | "reset";
}

/**
 * Theme-native button. Colors come from the app's --color-* variables so it
 * renders correctly across every theme (unlike the astryx neutral Button).
 * API mirrors the astryx Button (label/variant/icon/isDisabled) for drop-in use.
 */
export function Button({
  label,
  children,
  variant = "secondary",
  size = "md",
  icon,
  iconRight,
  isDisabled,
  isLoading,
  fullWidth,
  type = "button",
  className,
  style,
  disabled,
  ...rest
}: ButtonProps) {
  const cls = [
    "voco-btn",
    `voco-btn--${variant}`,
    `voco-btn--${size}`,
    fullWidth ? "voco-btn--fullWidth" : "",
    className || "",
  ]
    .filter(Boolean)
    .join(" ");

  return (
    <button type={type} className={cls} style={style} disabled={isDisabled || disabled || isLoading} {...rest}>
      {isLoading ? <span className="voco-spin" /> : icon}
      {label ?? children}
      {iconRight}
    </button>
  );
}

export default Button;
