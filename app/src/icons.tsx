// Small line icons (24-unit grid, stroke = currentColor). Inline JSX, so nothing is fetched
// and the CSP has nothing to allow.

const PATHS = {
  folder: "M3 7.5A2.5 2.5 0 0 1 5.5 5H9l2 2h7.5A2.5 2.5 0 0 1 21 9.5v7a2.5 2.5 0 0 1-2.5 2.5h-13A2.5 2.5 0 0 1 3 16.5z",
  disc: "M12 21a9 9 0 1 0 0-18 9 9 0 0 0 0 18zM12 14.5a2.5 2.5 0 1 0 0-5 2.5 2.5 0 0 0 0 5z",
  target: "M4 7.5 12 3l8 4.5v9L12 21l-8-4.5zM4 7.5l8 4.5 8-4.5M12 12v9",
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
  file: "M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8zM14 3v5h5",
  up: "M12 19V5M6 11l6-6 6 6",
  power: "M12 3v8M7.1 6.2a7.5 7.5 0 1 0 9.8 0",
  scale: "M12 4v16M8 20h8M5 7h14M5 7l-3 6a3 3 0 0 0 6 0zM19 7l-3 6a3 3 0 0 0 6 0z",
  compress: "M12 3v6M9 6l3 3 3-3M12 21v-6M9 18l3-3 3 3M5 12h14",
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
