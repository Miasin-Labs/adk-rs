module.exports = {
  content: ["./index.html", "./src/**/*.rs"],
  theme: {
    extend: {
      colors: {
        page: "#09090b",
        panel: "#111118",
        panelSoft: "#181824",
        adk: "#8b5cf6",
        flow: "#22d3ee",
      },
      fontFamily: {
        sans: ["Google Sans", "ui-sans-serif", "system-ui", "sans-serif"],
        mono: ["Google Sans Mono", "ui-monospace", "SFMono-Regular", "monospace"],
      },
    },
  },
  plugins: [],
};
