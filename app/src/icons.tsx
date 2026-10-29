// Small line icons (24-unit grid, stroke = currentColor). Inline JSX, so nothing is fetched
// and the CSP has nothing to allow.

const PATHS = {
  folder: "M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2h7.5A2.5 2.5 0 0 1 21 9.5v7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 16.5z",
  disc: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 14.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z",
  drive: "M3 13.5 5.6 6.2A1.8 1.8 0 0 1 7.3 5h9.4a1.8 1.8 0 0 1 1.7 1.2L21 13.5v4a1.5 1.5 0 0 1-1.5 1.5h-15A1.5 1.5 0 0 1 3 17.5zM3 13.5h18M16.5 16.2v.3",
  target: "M4 7.5 12 3l8 4.5v9L12 21l-8-4.5zM4 7.5l8 4.5 8-4.5M12 12v9",
  /** A package (the Package installer): the box with its tape. */
  package: "M4 7.5 12 3l8 4.5v9L12 21l-8-4.5zM4 7.5l8 4.5 8-4.5M12 12v9M8 5.25l8 4.5",
  bolt: "M13 3 5 13.5h6L10 21l8-10.5h-6z",
  check: "M5 12.5 10 17.5 19 7",
  checkCircle: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM8 12.3l2.7 2.7L16 9.7",
  warn: "M12 4 21 19.5H3zM12 10v4.5M12 17.2v.3",
  alert: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 7.5v5.5M12 16.3v.4",
  minus: "M7 12h10",
  x: "M7 7l10 10M17 7 7 17",
  table: "M4 5h16v14H4zM4 10h16M4 15h16M10 5v14",
  jobs: "M4 6h16M4 12h10M4 18h7M17 15v6M14 18h6",
  info: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 11v5.5M12 7.7v.3",
  finder: "M4 6.5A2.5 2.5 0 0 1 6.5 4h11A2.5 2.5 0 0 1 20 6.5v11a2.5 2.5 0 0 1-2.5 2.5h-11A2.5 2.5 0 0 1 4 17.5zM12 4v16M8 9v1.5M16 9v1.5M8.5 15.5c2 1.3 5 1.3 7 0",
  // A zipped file (zipper teeth down the top) holding a disc: an image or an archive.
  archive: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M10 5h1.5M10 9h1.5M8.5 7H10M8.5 11H10M12 19a2.8 2.8 0 1 0 0-5.6 2.8 2.8 0 0 0 0 5.6zM12 16.2v.01",
  file: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5",
  up: "M12 19V5M6 11l6-6 6 6",
  download: "M12 4v11M7 10l5 5 5-5M5 20h14",
  power: "M12 3v8M7.1 6.2a7.5 7.5 0 1 0 9.8 0",
  scale: "M12 4v16M8 20h8M5 7h14M5 7l-3 6a3 3 0 0 0 6 0zM19 7l-3 6a3 3 0 0 0 6 0z",
  compress: "M12 3v6M9 6l3 3 3-3M12 21v-6M9 18l3-3 3 3M5 12h14",
  lock: "M6.5 11h11a1.5 1.5 0 0 1 1.5 1.5v6a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 18.5v-6A1.5 1.5 0 0 1 6.5 11zM8.5 11V8a3.5 3.5 0 0 1 7 0v3",
  chevron: "M7 10l5 5 5-5",
  library: "M5 4v16M10 4v16M15 5l4 .8-3 14.5-4-.8zM3 20h8",
  search: "M10.5 17a6.5 6.5 0 1 0 0-13 6.5 6.5 0 0 0 0 13zM15.5 15.5 20 20",
  refresh: "M19 12a7 7 0 1 1-2.1-5M19 4.5V8h-3.5",
  grid: "M4 4h6.5v6.5H4zM13.5 4H20v6.5h-6.5zM4 13.5h6.5V20H4zM13.5 13.5H20V20h-6.5z",
  list: "M9 6h11M9 12h11M9 18h11M4.5 6h.01M4.5 12h.01M4.5 18h.01",
  convert: "M4 8h13M13 4l4 4-4 4M20 16H7M11 12l-4 4 4 4",
  pencil: "M4 20h4L19 9a2.8 2.8 0 0 0-4-4L4 16zM13.5 6.5l4 4",
  trash: "M4 7h16M9.5 7V4.5h5V7M6.5 7l1 12.5h9l1-12.5M10 11v5M14 11v5",
  undo: "M9 14 4 9l5-5M4 9h10.5a5.5 5.5 0 0 1 0 11H11",
  "folder-plus":
    "M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2h7.5A2.5 2.5 0 0 1 21 9.5v7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 16.5zM12 10.5v5M9.5 13h5",
  "folder-in":
    "M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2h7.5A2.5 2.5 0 0 1 21 9.5v7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 16.5zM12 10v5.5M9.5 13l2.5 2.5 2.5-2.5",
  // Three dots in a row: a menu of more actions.
  dots: "M5.5 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM12 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2zM18.5 13a1 1 0 1 0 0-2 1 1 0 0 0 0 2z",
  // A folder with an arrow going in: Move to another folder.
  move: "M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2h7.5A2.5 2.5 0 0 1 21 9.5v7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 16.5zM8 13h7M12.5 10.5 15 13l-2.5 2.5",
  plus: "M12 5v14M5 12h14",
  // An arrow pointing on (mirrored in RTL by its CSS): File Transfer, from what to where.
  arrow: "M5 12h14M13 6l6 6-6 6",
  // Lines running on with arrows: streamed, nothing staged (About's features).
  stream: "M4 7h11M4 12h16M4 17h11M15 4l3 3-3 3M15 14l3 3-3 3",
  // A memory card, its contacts on top: USB sticks and SD cards.
  sdcard: "M7 3h9l3 3v13.5a1.5 1.5 0 0 1-1.5 1.5h-11A1.5 1.5 0 0 1 5 19.5v-15A1.5 1.5 0 0 1 6.5 3zM9 3v4M12 3v4M15 3v4",
  heart: "M12 20s-7.5-4.6-7.5-10A4.5 4.5 0 0 1 12 7.4 4.5 4.5 0 0 1 19.5 10c0 5.4-7.5 10-7.5 10z",
  "file-plus": "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5M12 11.5v5M9.5 14h5",
} as const;

export type IconName = keyof typeof PATHS;

export function Icon({ name }: { name: IconName }) {
  return (
    <svg
      className="icon"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d={PATHS[name]} />
    </svg>
  );
}
