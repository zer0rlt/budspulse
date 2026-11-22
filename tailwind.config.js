/** @type {import('tailwindcss').Config} */
// Material 3 tokens are CSS variables switched per brand theme (see src/styles/theme.css).
const tok = (name) => `rgb(var(--md-${name}) / <alpha-value>)`;

export default {
  content: ["./index.html", "./toast.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        primary: tok("primary"),
        "on-primary": tok("on-primary"),
        "primary-container": tok("primary-container"),
        "on-primary-container": tok("on-primary-container"),
        secondary: tok("secondary"),
        "secondary-container": tok("secondary-container"),
        "on-secondary-container": tok("on-secondary-container"),
        surface: tok("surface"),
        "surface-dim": tok("surface-dim"),
        "surface-c": tok("surface-container"),
        "surface-c-high": tok("surface-container-high"),
        "surface-c-highest": tok("surface-container-highest"),
        "on-surface": tok("on-surface"),
        "on-surface-variant": tok("on-surface-variant"),
        outline: tok("outline"),
        "outline-variant": tok("outline-variant"),
        success: tok("success"),
        warning: tok("warning"),
        danger: tok("danger"),
      },
      fontFamily: {
        sans: ["var(--font-sans)", "system-ui", "sans-serif"],
        display: ["var(--font-display)", "var(--font-sans)", "monospace"],
        mono: ["JetBrains Mono", "Cascadia Code", "Consolas", "monospace"],
      },
      borderRadius: { xl2: "1.25rem", xl3: "1.75rem" },
    },
  },
  plugins: [],
};
