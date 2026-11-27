// Small line icons for toolbars. They draw in `currentColor` at 16 px unless a size is given;
// the agent and human glyphs carry classes so a toolbar can tint them when active.
import type { ReactNode } from "react";

function Svg({
  size = 16,
  width,
  viewBox = "0 0 16 16",
  children,
}: {
  size?: number;
  width?: number;
  viewBox?: string;
  children: ReactNode;
}) {
  return (
    <svg
      width={width ?? size}
      height={size}
      viewBox={viewBox}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.4"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {children}
    </svg>
  );
}

/** A small robot head, drawn with its top-left corner at (x, y). */
function AgentGlyph({ x = 0, y = 0 }: { x?: number; y?: number }) {
  return (
    <g className="glyph-agent" transform={`translate(${x} ${y})`}>
      <rect x="2.5" y="5" width="11" height="8.5" rx="2.5" />
      <path d="M8 5V2.8" />
      <circle cx="8" cy="2.2" r="0.7" fill="currentColor" stroke="none" />
      <circle cx="5.9" cy="9.2" r="0.95" fill="currentColor" stroke="none" />
      <circle cx="10.1" cy="9.2" r="0.95" fill="currentColor" stroke="none" />
    </g>
  );
}

/** A person (head and shoulders), drawn with its top-left corner at (x, y). */
function HumanGlyph({ x = 0, y = 0 }: { x?: number; y?: number }) {
  return (
    <g className="glyph-human" transform={`translate(${x} ${y})`}>
      <circle cx="8" cy="5" r="2.7" />
      <path d="M2.8 14c0-2.9 2.3-5.1 5.2-5.1s5.2 2.2 5.2 5.1" />
    </g>
  );
}

export const AgentIcon = () => (
  <Svg>
    <AgentGlyph />
  </Svg>
);

export const HumanIcon = () => (
  <Svg>
    <HumanGlyph />
  </Svg>
);

/** The agent and the person side by side. */
export const TogetherIcon = () => (
  <Svg width={27} viewBox="0 0 27 16">
    <AgentGlyph x={-1.5} y={0.5} />
    <HumanGlyph x={12} />
  </Svg>
);

export const EyeIcon = () => (
  <Svg>
    <path d="M1.5 8s2.4-4.5 6.5-4.5S14.5 8 14.5 8s-2.4 4.5-6.5 4.5S1.5 8 1.5 8z" />
    <circle cx="8" cy="8" r="1.9" />
  </Svg>
);

export const PencilIcon = () => (
  <Svg>
    <path d="M10.8 2.8l2.4 2.4L6 12.4l-3.1.7.7-3.1z" />
    <path d="M9.4 4.2l2.4 2.4" />
  </Svg>
);

export const MoreIcon = () => (
  <Svg>
    <circle cx="3.5" cy="8" r="1.1" fill="currentColor" stroke="none" />
    <circle cx="8" cy="8" r="1.1" fill="currentColor" stroke="none" />
    <circle cx="12.5" cy="8" r="1.1" fill="currentColor" stroke="none" />
  </Svg>
);

/** Newest first (arrow down) or oldest first (arrow up). */
export const SortIcon = ({ oldestFirst }: { oldestFirst: boolean }) => (
  <Svg>
    {oldestFirst ? (
      <path d="M5 13V3M2.5 5.5L5 3l2.5 2.5M9 5h5M9 8h4M9 11h3" />
    ) : (
      <path d="M5 3v10M2.5 10.5L5 13l2.5-2.5M9 5h3M9 8h4M9 11h5" />
    )}
  </Svg>
);

function CalendarFrame({ children }: { children: ReactNode }) {
  return (
    <Svg>
      <rect x="2" y="3" width="12" height="11" rx="2" />
      <path d="M2 6.5h12M5.5 1.8v2.4M10.5 1.8v2.4" />
      {children}
    </Svg>
  );
}

/** One day picked out of the calendar. */
export const DayIcon = () => (
  <CalendarFrame>
    <rect x="8.6" y="8.6" width="3" height="3" rx="0.7" fill="currentColor" stroke="none" />
  </CalendarFrame>
);

/** One week (a row) of the calendar. */
export const WeekIcon = () => (
  <CalendarFrame>
    <rect x="4.2" y="8.9" width="7.6" height="2.4" rx="0.8" fill="currentColor" stroke="none" />
  </CalendarFrame>
);

/** The whole month. */
export const MonthIcon = () => (
  <CalendarFrame>
    {[5, 8, 11].flatMap((x) =>
      [8.9, 11.6].map((y) => (
        <circle key={`${x}-${y}`} cx={x} cy={y} r="0.85" fill="currentColor" stroke="none" />
      )),
    )}
  </CalendarFrame>
);

export const ProjectsIcon = () => (
  <Svg>
    <path d="M2 4.5A1.5 1.5 0 0 1 3.5 3h2.8l1.4 1.5h4.8A1.5 1.5 0 0 1 14 6v6a1.5 1.5 0 0 1-1.5 1.5h-9A1.5 1.5 0 0 1 2 12z" />
  </Svg>
);

/** Jump back to the current period. */
export const TargetIcon = () => (
  <Svg size={14}>
    <circle cx="8" cy="8" r="5.5" />
    <circle cx="8" cy="8" r="1.6" fill="currentColor" stroke="none" />
  </Svg>
);

/** A page with a plus: write a note. */
export const NoteAddIcon = () => (
  <Svg>
    <path d="M9.5 2H4.5A1.5 1.5 0 0 0 3 3.5v9A1.5 1.5 0 0 0 4.5 14h7a1.5 1.5 0 0 0 1.5-1.5V5.5z" />
    <path d="M9.5 2v3.5H13" />
    <path d="M8 7.6v4M6 9.6h4" />
  </Svg>
);

/** The "+" on add buttons. */
export function PlusIcon() {
  return (
    <svg
      width="11"
      height="11"
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="2.6"
      strokeLinecap="round"
      aria-hidden
    >
      <path d="M12 5v14M5 12h14" />
    </svg>
  );
}
