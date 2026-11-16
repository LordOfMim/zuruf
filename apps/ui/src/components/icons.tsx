/**
 * Zuruf's icon set. Every icon in the app and the page overlay comes from this one table.
 *
 * Palermo publishes no icon library, so the set is drawn to sit next to its type and controls.
 * Rules for every icon, old and new:
 *
 * - 20 × 20 grid, drawn inside a 3 px safe margin (live area 14 × 14, optical overshoot allowed
 *   for circles and pointed shapes).
 * - Outline only: 1.5 px stroke, round caps and joins, no fills. A "dot" is a zero-length stroke.
 * - Corner radius of drawn rectangles: 1.5 px, matching the controls' small radius at icon scale.
 * - One path per icon, `currentColor` only. Colour comes from the text it sits in.
 * - Sizes in use: 12 (inside menus and chips), 14 (inside small buttons), 16 (default), 18 (rail).
 * - An icon never stands alone as the only label of an action unless the button has a title
 *   or aria-label. State is shown with the glyph dots, not with icons.
 * - No emoji, no icon fonts, no second icon source.
 */
const ICONS = {
  // Capturing and commenting
  viewport: "M3 5.5A1.5 1.5 0 0 1 4.5 4h11A1.5 1.5 0 0 1 17 5.5v9a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5zM3 7.5h14",
  region: "M4 7V5a1 1 0 0 1 1-1h2M13 4h2a1 1 0 0 1 1 1v2M16 13v2a1 1 0 0 1-1 1h-2M7 16H5a1 1 0 0 1-1-1v-2",
  dom: "M7.5 6 4 10l3.5 4M12.5 6 16 10l-3.5 4",
  text: "M5 5h10M5 9h10M5 13h6",
  mic: "M10 3a2 2 0 0 0-2 2v4a2 2 0 1 0 4 0V5a2 2 0 0 0-2-2zM5.5 9.5a4.5 4.5 0 0 0 9 0M10 14v3",

  // Actions
  send: "M4 10h11M11 5.5l4.5 4.5-4.5 4.5",
  play: "M7 4.8v10.4l8.5-5.2z",
  stop: "M6.5 5h7A1.5 1.5 0 0 1 15 6.5v7a1.5 1.5 0 0 1-1.5 1.5h-7A1.5 1.5 0 0 1 5 13.5v-7A1.5 1.5 0 0 1 6.5 5z",
  retry: "M15.5 10a5.5 5.5 0 1 1-1.7-4M15.5 4v3h-3",
  plus: "M10 4.5v11M4.5 10h11",
  close: "M5.5 5.5l9 9M14.5 5.5l-9 9",
  check: "M4.5 10.5l3.5 3.5 7.5-8",
  trash: "M5 6.5h10M8 6.5V5h4v1.5M6.5 6.5l.6 9h5.8l.6-9",
  external: "M8 5H5.5A1.5 1.5 0 0 0 4 6.5v8A1.5 1.5 0 0 0 5.5 16h8a1.5 1.5 0 0 0 1.5-1.5V12M11 4h5v5M16 4l-7 7",
  hand: "M7 10V5.5a1.2 1.2 0 0 1 2.4 0V9M9.4 9V4.7a1.2 1.2 0 0 1 2.4 0V9M11.8 9V5.7a1.2 1.2 0 0 1 2.4 0v5.8c0 2.8-1.9 4.5-4.5 4.5-2 0-3.2-.9-4.2-2.6L4 10.9a1.1 1.1 0 0 1 1.9-1.1L7 11.4",
  chevron: "M6 8l4 4 4-4",

  // Places and things
  review: "M10 4c-3.5 0-6 3.2-7 6 1 2.8 3.5 6 7 6s6-3.2 7-6c-1-2.8-3.5-6-7-6zM10 12.2a2.2 2.2 0 1 0 0-4.4 2.2 2.2 0 0 0 0 4.4z",
  tasks: "M4 5.5h1.5M8 5.5h8M4 10h1.5M8 10h8M4 14.5h1.5M8 14.5h8",
  browser: "M3 5.5A1.5 1.5 0 0 1 4.5 4h11A1.5 1.5 0 0 1 17 5.5v9a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5zM3 7.5h14M5.3 5.8h.01M7.3 5.8h.01",
  settings: "M4 6h7M14 6h2M4 14h2M9 14h7M11 4.5v3M6 12.5v3",
  folder: "M3 6.5A1.5 1.5 0 0 1 4.5 5h3l1.7 2h6.3A1.5 1.5 0 0 1 17 8.5v6a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5z",
  terminal: "M3 5.5A1.5 1.5 0 0 1 4.5 4h11A1.5 1.5 0 0 1 17 5.5v9a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 3 14.5zM6.5 8 9 10l-2.5 2M11 12h2.5",
  globe: "M10 3.5a6.5 6.5 0 1 0 0 13 6.5 6.5 0 0 0 0-13zM3.5 10h13M10 3.5c-1.8 1.9-2.6 4.1-2.6 6.5s.8 4.6 2.6 6.5c1.8-1.9 2.6-4.1 2.6-6.5s-.8-4.6-2.6-6.5z",
  warn: "M10 4 3.5 15.5h13zM10 8.5v3.2M10 13.6h.01",
} as const;

export type IconName = keyof typeof ICONS;

export function Icon(props: { name: IconName; size?: number; class?: string }) {
  return (
    <svg
      class={`icon ${props.class ?? ""}`}
      width={props.size ?? 16}
      height={props.size ?? 16}
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      stroke-width="1.5"
      stroke-linecap="round"
      stroke-linejoin="round"
      aria-hidden="true"
    >
      <path d={ICONS[props.name]} />
    </svg>
  );
}
