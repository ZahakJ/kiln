# @kiln/home

Kiln Home, the start screen (PLAN.md §1.6, brand mockups `docs/design/mockups/home-*.png` and `phone-*.png`): the left rail with the ember **New** menu, Home / Recent / Shared with me / Pinned / Templates / Open file, the app filters and the local-first card; the greeting; the four arch-shaped app doors (Document, Workbook, Deck, Notebook); the "Coming from Microsoft Office?" banner; the Recent / Pinned / Shared lists; the templates strip; search over files and templates; and the phone layout with the bottom tab bar.

**Public API.** `KilnHome` (props: `recent`, `shared`, `templates`, `storage`, `userName`, `initials`, `creatable`, and the callbacks `onCreate`, `onOpenFile`, `onOpenRecent`, `onTogglePin`, `onOpenSettings`, `onTour`, `onImportSettings`…), `relativeTime`, `greeting`, `kindApp`/`appKind`, `HOME_APPS`, `DOOR_MASK`.

Presentation only (PLAN.md §3.7): files come from the core's recents through the host (`HostOp::Recents`), templates from the core's template index (WP-85), and Home keeps no state beyond the selected list, the app filter and the search text. `apps/web/src/routes/home` wires it to the client.
