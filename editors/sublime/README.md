# Sublime Text syntax for ALGOL26

## Installation

Sublime Text loads syntax files from its **Packages** directory, not
from arbitrary folders in your project. The `ALGOL26.sublime-syntax`
file in this directory is the *source* — it must be installed into
Sublime's Packages directory before Sublime will use it.

### Recommended: symlink

    ln -s ~/dev/algol26/editors/sublime/ALGOL26.sublime-syntax \
          ~/.config/sublime-text/Packages/User/ALGOL26.sublime-syntax

Test it: open a `.gol` file and check that `pub` and `and` are
colored. If they aren't, or if the file shows as `Plain Text`, the
symlink isn't being followed — fall back to copy.

### Fallback: copy

    cp ~/dev/algol26/editors/sublime/ALGOL26.sublime-syntax \
       ~/.config/sublime-text/Packages/User/ALGOL26.sublime-syntax

You must re-copy after every edit to the repo file.

## Why this is necessary

Sublime's syntax loader only reads `Packages/`. There is no
`.sublime-project` setting that makes Sublime read
`editors/sublime/` directly.

## Verifying which file Sublime is loading

Cursor on `pub`, `Ctrl+Alt+Shift+P` (Show Scope Name). Expect
`source.algol26 storage.modifier`. If it says `variable.other`,
Sublime is loading a stale copy from `Packages/User/`.

## Tree-sitter note

Sublime uses this `.sublime-syntax` file, not the
`tree-sitter-algol26/` grammar. See `LANGUAGE_GAPS.md` G33 for
tree-sitter-aware editors.
