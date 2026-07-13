import type { ButtonHTMLAttributes, ReactNode } from "react";
import "./ui.css";

interface IconButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  children: ReactNode;
  /** Square size in px (default 32). */
  size?: number;
}

/** Square, theme-native icon button (copy/export/close/etc.). */
export function IconButton({ children, size = 32, className, style, type = "button", ...rest }: IconButtonProps) {
  return (
    <button
      type={type}
      className={["voco-iconbtn", className || ""].filter(Boolean).join(" ")}
      style={{ width: size, height: size, ...style }}
      {...rest}
    >
      {children}
    </button>
  );
}

export default IconButton;
