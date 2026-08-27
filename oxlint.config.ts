import { defineConfig } from "oxlint";

export default defineConfig({
  ignorePatterns: [
    // Agent tooling: installed skills and generated configuration, not source.
    ".agent/**",
    ".agents/**",
    ".claude/**",
    ".codex/**",
    ".continue/**",
    ".cursor/**",
    ".gemini/**",
    ".opencode/**",
    ".pi/**",
    ".roo/**",
    ".windsurf/**",
    // The vendored plugin lints this repository; it is not part of it.
    "tools/oxlint/anti-slop/**",
    // Build output and vendored assets, in any workspace.
    "**/dist/**",
    "**/target/**",
    "**/.next/**",
    "**/public/**",
    "**/node_modules/**",
    // Written by @tanstack/router-plugin on every build.
    "**/routeTree.gen.ts",
    // The web app is linted by its own Biome config.
    "apps/web/**",
  ],
  jsPlugins: [
    { name: "anti-slop", specifier: "./tools/oxlint/anti-slop/index.ts" },
    {
      name: "anti-slop-effect",
      specifier: "./tools/oxlint/anti-slop/effect/index.ts",
    },
  ],
  rules: {
    "anti-slop/no-chained-type-assertions": "error",
    "anti-slop/no-conditional-empty-object-spread": "error",
    "anti-slop/no-known-value-widening": "error",
    "anti-slop/no-module-mocking": "error",
    "anti-slop/no-object-parameters": "error",
    "anti-slop/no-reflect-apply": "error",
    "anti-slop/no-reflect-get": "error",
    "anti-slop/no-runtime-typeof": "error",
    "anti-slop/no-shape-in-symbol-names": "error",
    "anti-slop/no-unknown-parameters": "error",
    "anti-slop/no-unknown-returns": "error",
    "anti-slop/no-unknown-type-aliases": "error",
    "anti-slop/no-unsafe-dictionary-type": "error",
    "anti-slop/no-widen-then-assert": "error",
    "anti-slop/require-safety-comment-for-type-assertion": "error",

    // Effect is a direct dependency, so the opt-in Effect group applies.
    "anti-slop-effect/no-service-constructor-imports": "error",

    // Cyclomatic complexity. 15 rather than ESLint's default of 20: a function
    // with more than fifteen independent paths through it is asking to be read
    // twice, and this codebase has none.
    "eslint/complexity": ["error", { max: 15 }],
  },
});
