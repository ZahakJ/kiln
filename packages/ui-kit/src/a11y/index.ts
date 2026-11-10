/**
 * Accessibility helpers of the chrome: live announcements, F6 region cycling, the
 * Accessible text view and the announcements of a document window.
 */
export { announce, lastAnnouncement, type Politeness, prepareAnnouncer } from './announce.ts';
export {
  cycleRegions,
  focusRegion,
  REGION_ORDER,
  type RegionId,
  regionsOf,
  useRegionCycle,
} from './regions.ts';
export {
  selectionKey,
  textAnnouncement,
  useSelectionAnnouncements,
} from './selection.ts';
export {
  AccessibleTextView,
  type AccessibleTextViewProps,
  offsetIn,
  pointIn,
  type TextViewApp,
} from './TextView.tsx';
