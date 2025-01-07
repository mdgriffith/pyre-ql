# Pyre VSCode support

To use this locally:

1. cwd to this directory.
2. Run `bun install` and `bun run build` in this directory.
2. Link this directory to the VSCode or Cursor extensions directory.
    VSCode: `ln -s "$(pwd)" ~/.vscode/extensions/pyre-lang`
    Cursor: `ln -s "$(pwd)" ~/.vscode/extensions/pyre-lang`

In VSCode, enable the Pyre language support extension.
You might need to reload the window, which can be done via Cmd+Shift+P and executing `Developer: Reload Window`.