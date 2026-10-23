// Inline stroke icons. No icon library, to stay inside the bundle budget (docs/PERF-BUDGET.md).
import type { ReactNode } from "react";

function Svg({ children, size = 20 }: { children: ReactNode; size?: number }) {
  return (
    <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.8}
      strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{children}</svg>
  );
}

export const Logo = () => (
  <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#0a0b10" strokeWidth={2.4} strokeLinecap="round" aria-hidden="true">
    <path d="M5 7h14M5 12h10M5 17h6" />
  </svg>
);
export const Gauge = () => <Svg><path d="M12 13l4-4" /><path d="M3.3 15a9 9 0 1 1 17.4 0" /></Svg>;
export const Grid = () => <Svg><rect x="3" y="3" width="7" height="7" rx="2" /><rect x="14" y="3" width="7" height="7" rx="2" /><rect x="3" y="14" width="7" height="7" rx="2" /><rect x="14" y="14" width="7" height="7" rx="2" /></Svg>;
export const Clock = () => <Svg><path d="M3 12a9 9 0 1 0 3-6.7L3 8" /><path d="M3 3v5h5" /><path d="M12 7v5l3 2" /></Svg>;
export const Refresh = ({ size = 16 }: { size?: number }) => <Svg size={size}><path d="M21 12a9 9 0 1 1-2.6-6.4" /><path d="M21 4v5h-5" /></Svg>;
export const Close = () => <Svg><path d="M6 6l12 12M18 6L6 18" /></Svg>;
export const Power = () => <Svg size={18}><path d="M12 2v10" /><path d="M18.4 6.6a9 9 0 1 1-12.8 0" /></Svg>;
export const Record = () => <Svg size={18}><circle cx="12" cy="12" r="9" /><circle cx="12" cy="12" r="2.5" fill="currentColor" /></Svg>;
export const Mouse = () => <Svg size={18}><rect x="6" y="2" width="12" height="20" rx="6" /><path d="M12 6v4" /></Svg>;
export const Store = () => <Svg size={18}><path d="M21 15V6a2 2 0 0 0-2-2H5a2 2 0 0 0-2 2v9" /><path d="M3 15h18v3a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /><path d="M12 8v4M10 10h4" /></Svg>;
export const Pad = () => <Svg size={18}><path d="M6 12h4M8 10v4" /><circle cx="16" cy="11" r="0.8" fill="currentColor" /><circle cx="18" cy="13" r="0.8" fill="currentColor" /><path d="M7 6h10a5 5 0 0 1 5 5v1a5 5 0 0 1-9 3h-2a5 5 0 0 1-9-3v-1a5 5 0 0 1 5-5z" /></Svg>;
export const Bolt = () => <Svg size={18}><path d="M13 2L4 14h7l-1 8 9-12h-7z" /></Svg>;
export const Monitor = () => <Svg size={18}><rect x="2" y="4" width="20" height="13" rx="2" /><path d="M8 21h8M12 17v4" /></Svg>;
export const Chip = () => <Svg size={18}><rect x="5" y="5" width="14" height="14" rx="2" /><path d="M9 1v4M15 1v4M9 19v4M15 19v4M1 9h4M1 15h4M19 9h4M19 15h4" /></Svg>;
export const Disk = () => <Svg size={18}><ellipse cx="12" cy="6" rx="8" ry="3" /><path d="M4 6v12c0 1.7 3.6 3 8 3s8-1.3 8-3V6" /></Svg>;
export const Check = () => <Svg size={16}><path d="M20 6L9 17l-5-5" /></Svg>;
