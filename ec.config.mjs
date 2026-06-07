import { pluginCollapsibleSections } from "@expressive-code/plugin-collapsible-sections";
import { pluginLineNumbers } from "@expressive-code/plugin-line-numbers";

// Expressive Code config lives here (not in astro.config.mjs) so the Starlight
// `<Code>` component works: plugins are functions and can't be serialized out of
// the Astro config. Starlight auto-loads this file.
export default {
  themes: ["gruvbox-dark-medium", "gruvbox-light-medium"],
  styleOverrides: {
    codeFontFamily:
      "'IBM Plex Mono', ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, monospace",
  },
  // Collapse boilerplate inside code fences with `collapse={4-12}` so each
  // TS↔Rust comparison focuses on the lines that actually differ.
  // Line numbers ON for all code blocks; opt out per fence with `showLineNumbers=false`.
  plugins: [pluginCollapsibleSections(), pluginLineNumbers()],
  defaultProps: {
    showLineNumbers: true,
  },
};
