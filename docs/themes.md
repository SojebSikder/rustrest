# Themes

Rustrest's themes work like [Zed's](https://zed.dev/docs/themes): a theme is a JSON file in **Zed's theme format**, themes can be installed as **extensions**, and the `"theme"` setting can follow the OS light/dark mode. A Zed theme file or a Zed theme extension works in Rustrest unchanged.

## Contents

- [Picking a theme](#picking-a-theme)
- [The `theme` setting](#the-theme-setting)
- [Overriding a theme](#overriding-a-theme)
- [Where themes come from](#where-themes-come-from)
- [Writing a theme](#writing-a-theme)
- [Theme keys Rustrest reads](#theme-keys-rustrest-reads)
- [Theme extensions](#theme-extensions)
- [Publishing a theme extension](#publishing-a-theme-extension)

## Picking a theme

- **Theme selector**: press `Ctrl+K Ctrl+T`, or choose **Go → Select Theme...**, or run **Theme Selector: Toggle** from the command palette (`Ctrl+Shift+P`). Use ↑/↓ to preview themes live, Enter to keep one, and Esc to go back to the one you had.
- **Settings → Theme**: choose **Light**, **Dark** or **System** appearance, and pick a theme for each appearance. From here you can also **Import Theme...**, **Open Themes Folder** and **Reload** themes.

## The `theme` setting

The choice is saved to `settings.json` in Rustrest's data directory: `%APPDATA%\Rustrest\` on Windows, `~/Library/Application Support/Rustrest/` on macOS, or `~/.local/share/Rustrest/` on Linux. The setting has the same two forms as in Zed:

```jsonc
// one theme, always
"theme": "One Dark"

// follow the OS: "light", "dark" or "system"
"theme": {
  "mode": "system",
  "light": "One Light",
  "dark": "One Dark"
}
```

If you edit `settings.json` by hand, Rustrest applies the change straight away, without a restart. While the file doesn't parse, for example halfway through an edit, Rustrest ignores it rather than resetting your settings. If a theme named in the setting isn't installed (say its extension was removed), Rustrest falls back to the built-in **Light** or **Dark** theme.

## Overriding a theme

`theme_overrides` changes individual keys of a theme without editing its file. It is keyed by theme name, and the keys are the ones described under [Writing a theme](#writing-a-theme):

```json
"theme_overrides": {
  "One Dark": {
    "editor.background": "#1e2127ff",
    "syntax": {
      "comment": { "font_style": "italic" }
    }
  }
}
```

Entries under `syntax` are merged one token at a time, so an override only has to name the tokens it changes.

## Where themes come from

Themes are loaded from these sources, in order. A theme with the same name as an earlier one replaces it, so your own themes can shadow built-in or extension themes.

1. **iced's built-in themes**: Light, Dark, Dracula, Nord, Solarized, Gruvbox, Catppuccin, Tokyo Night, Kanagawa, Moonfly, Nightfly, Oxocarbon, Ferra.
2. **Bundled**: One Dark and One Light.
3. **Extensions**: `plugins/<id>/themes/*.json` for every enabled extension (see [Theme extensions](#theme-extensions)).
4. **Your themes folder**: `themes/*.json` in the data directory. **Open Themes Folder** opens it, and **Import Theme...** checks a file and copies it there.

Rustrest watches the themes folder, the extensions and `settings.json`. Saving a theme file restyles the app immediately, which makes writing a theme a live-preview loop. If a file fails to parse, Rustrest shows the error in a toast and in **Settings → Theme**.

## Writing a theme

A theme file is a **theme family**: one or more themes that share an author. This is the [Zed theme schema](https://zed.dev/schema/themes/v0.2.0.json):

```json
{
  "$schema": "https://zed.dev/schema/themes/v0.2.0.json",
  "name": "Midnight",
  "author": "You",
  "themes": [
    {
      "name": "Midnight",
      "appearance": "dark",
      "style": {
        "background": "#15171cff",
        "editor.background": "#101216ff",
        "editor.foreground": "#c7ccd6ff",
        "text": "#e0e4ecff",
        "text.muted": "#8a90a0ff",
        "text.accent": "#7aa2f7ff",
        "border": "#2a2e38ff",
        "surface.background": "#181b21ff",
        "success": "#9ece6aff",
        "warning": "#e0af68ff",
        "error": "#f7768eff",
        "syntax": {
          "keyword": { "color": "#bb9af7ff", "font_style": null, "font_weight": null },
          "string": { "color": "#9ece6aff", "font_style": null, "font_weight": null },
          "comment": { "color": "#565f89ff", "font_style": "italic", "font_weight": null }
        }
      }
    }
  ]
}
```

- `appearance` is `"light"` or `"dark"`. It decides which slot the theme fills when `"mode"` is `"system"`.
- Colors are hex strings: `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
- Every key is optional. Keys you leave out, or set to `null`, fall back to values derived from the ones you set. A theme with only `editor.background`, `text` and `text.accent` still looks coherent.
- Keys Rustrest doesn't use are ignored, so a full Zed theme loads fine.

A complete example is [`assets/themes/one.json`](../assets/themes/one.json), the bundled One Dark and One Light.

## Theme keys Rustrest reads

**Base palette.** Every standard widget (buttons, inputs, pick lists, scrollbars, …) derives its look from these keys:

| Palette slot | Keys, first match wins |
| --- | --- |
| background | `editor.background`, `background` |
| text | `text`, `editor.foreground` |
| primary (accent) | `text.accent`, `icon.accent`, `border.focused` |
| success | `success`, `created` |
| warning | `warning`, `modified` |
| danger | `error`, `deleted` |

**Surfaces.** These keys replace the shades iced would otherwise generate from the background:

| Key | Used for |
| --- | --- |
| `surface.background` | boxed panels (sidebar, console, editors), cards |
| `elevated_surface.background` | modals, toasts, popups |
| `panel.background` | panels |
| `element.background` / `element.hover` / `element.selected` | inputs, hovered rows, selected rows (git file list) |
| `border` / `border.variant` | borders / subtle dividers and resize handles |
| `title_bar.background` | the menu bar |
| `status_bar.background` | the status bar |
| `tab.active_background` / `tab.inactive_background` | request tabs. Setting `tab.active_background` switches the active tab from an accent fill to a Zed-style tab with an accent outline |

**Text.** `text`, `text.muted` (hints, secondary labels), `text.placeholder`, `text.accent` (links, progress indicators).

**Status.** `success` and `error` color HTTP status codes and test pass/fail. `warning`, `info` and `hint` color console log levels. `created`, `modified`, `deleted`, `renamed`, `conflict` and `ignored` color the git status badges, and `modified` also colors the unsaved-changes dot.

**Terminal.** `terminal.background`, `terminal.foreground`, and `terminal.ansi.{black,red,green,yellow,blue,magenta,cyan,white}` with their `terminal.ansi.bright_*` variants. `players[0].selection` is the selection color.

**Syntax.** The `syntax` table colors the script editor. Each entry is `{ "color", "font_style": "italic" | null, "font_weight": 700 | null }`. Weights of 600 and above render bold. The tokens Rustrest recognizes are:

`comment`, `comment.doc`, `string`, `string.escape`, `string.regex`, `string.special`, `number`, `boolean`, `constant`, `keyword`, `operator`, `function`, `function.method`, `constructor`, `type`, `type.builtin`, `variable`, `variable.special`, `variable.parameter`, `property`, `attribute`, `tag`, `punctuation`, `punctuation.bracket`, `punctuation.delimiter`, `punctuation.special`, `embedded`, `label`, `link_uri`, `link_text`, `title`, `emphasis`, `emphasis.strong`, `enum`, `namespace`.

A theme without a `syntax` table uses Solarized on dark themes and InspiredGitHub on light ones.

## Theme extensions

A theme extension is a plugin folder with **no wasm**, just a manifest and a `themes/` folder, the same layout as a Zed theme extension:

```
midnight/
├── plugin.toml        # or a Zed extension.toml
└── themes/
    └── midnight.json  # every *.json in themes/ is loaded
```

`plugin.toml` needs only the metadata fields:

```toml
id = "midnight"
name = "Midnight"
version = "0.1.0"
author = "You"
description = "A dark theme"
```

Instead of `plugin.toml`, the folder may contain a Zed `extension.toml`, so a Zed theme extension repository installs as it is:

```toml
id = "midnight"
name = "Midnight"
version = "0.1.0"
schema_version = 1
authors = ["You <you@example.com>"]
description = "A dark theme"
repository = "https://github.com/you/midnight"
```

- **Install** it like any plugin: **Manage Plugins → Install Plugin Folder...**, or from the gallery. It then shows a **Theme extension** badge.
- **Disable** it with its checkbox to remove its themes from the theme selector.
- **Uninstall** it to delete it.

The theme list updates immediately in all three cases. A regular wasm plugin can ship themes too: add a `themes/` folder next to its `plugin.wasm`.

[`examples/theme-extension`](../examples/theme-extension) is a working example you can install directly.

## Publishing a theme extension

Publish it the same way as any plugin (see [Publishing to the plugin gallery](plugin-development.md#publishing-to-the-plugin-gallery)):

1. Zip the manifest and the `themes/` folder.
2. Attach the zip to a release.
3. Add an entry to the gallery `index.json`.

The installer finds the manifest (`plugin.toml` or `extension.toml`) anywhere in the zip.
