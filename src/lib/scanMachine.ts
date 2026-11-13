import type {
  NodeSummary,
  ScanEvent,
  ScanIssue,
  ScanProgress,
  ScanRequest,
  ScanSummary,
  ViewNode
} from "../types";

export interface ChildrenPage {
  items: NodeSummary[];
  total: number;
}

export interface Nav {
  history: number[];
  index: number;
}

/**
 * The scan shown by one window. `gen` identifies the scan the user asked for
 * last; every action from async work carries the gen it was started under,
 * and actions from an older gen are dropped.
 */
export type Scan =
  | { kind: "idle"; gen: number }
  | { kind: "authorizing"; gen: number; request: ScanRequest; scanId: string | null }
  | {
      kind: "scanning" | "cancelling";
      gen: number;
      request: ScanRequest;
      scanId: string | null;
      progress: ScanProgress | null;
      liveIssues: ScanIssue[];
    }
  | {
      kind: "loading";
      gen: number;
      request: ScanRequest;
      scanId: string;
      summary: ScanSummary;
      partial: boolean;
    }
  | {
      kind: "ready";
      gen: number;
      request: ScanRequest;
      scanId: string | null;
      summary: ScanSummary | null;
      /** null: nothing could be shown (stopped before any data, or the map failed to load). */
      view: ViewNode | null;
      children: ChildrenPage;
      issues: ScanIssue[];
      nav: Nav;
      partial: boolean;
    }
  | { kind: "failed"; gen: number; request: ScanRequest; message: string };

export type ScanKind = Scan["kind"];

export type ScanAction =
  | { type: "start"; gen: number; request: ScanRequest }
  | { type: "reset"; gen: number }
  | { type: "assigned"; gen: number; scanId: string }
  | { type: "startFailed"; gen: number; message: string }
  | { type: "event"; gen: number; event: ScanEvent }
  | { type: "cancel"; gen: number }
  | {
      type: "loaded";
      gen: number;
      view: ViewNode;
      children: ChildrenPage;
      issues: ScanIssue[];
    }
  | { type: "loadFailed"; gen: number }
  | {
      type: "view";
      gen: number;
      view: ViewNode;
      children: ChildrenPage;
      /** "push" a new history entry, keep it ("replace"), or move to an index. */
      nav: "push" | "replace" | number;
    }
  | { type: "more"; gen: number; nodeId: number; offset: number; page: ChildrenPage };

export const MAX_LIVE_ISSUES = 5000;
const EMPTY_CHILDREN: ChildrenPage = { items: [], total: 0 };

export const initialScan: Scan = { kind: "idle", gen: 0 };

/** The backend has work in flight for this scan. */
export function isBusy(scan: Scan): boolean {
  return (
    scan.kind === "authorizing" ||
    scan.kind === "scanning" ||
    scan.kind === "cancelling" ||
    scan.kind === "loading"
  );
}

/** Stop is meaningful only before the backend has finished. */
export function canStop(scan: Scan): boolean {
  return scan.kind === "authorizing" || scan.kind === "scanning";
}

function onEvent(scan: Scan, event: ScanEvent): Scan {
  if (
    scan.kind !== "authorizing" &&
    scan.kind !== "scanning" &&
    scan.kind !== "cancelling"
  ) {
    return scan;
  }
  const scanId = event.scanId ?? scan.scanId;
  switch (event.event) {
    case "authorizing":
      return scan.kind === "authorizing" ? { ...scan, scanId } : scan;
    case "started":
      if (scan.kind === "authorizing") {
        return {
          kind: "scanning",
          gen: scan.gen,
          request: scan.request,
          scanId,
          progress: null,
          liveIssues: []
        };
      }
      return { ...scan, scanId };
    case "progress":
      return scan.kind !== "authorizing" && event.progress
        ? { ...scan, scanId, progress: event.progress }
        : scan;
    case "issue":
      return scan.kind !== "authorizing" &&
        event.issue &&
        scan.liveIssues.length < MAX_LIVE_ISSUES
        ? { ...scan, liveIssues: [...scan.liveIssues, event.issue] }
        : scan;
    case "completed":
    case "cancelled": {
      const partial =
        event.event === "cancelled" || event.summary?.status === "cancelled";
      if (event.summary && scanId) {
        return {
          kind: "loading",
          gen: scan.gen,
          request: scan.request,
          scanId,
          summary: event.summary,
          partial
        };
      }
      return {
        kind: "ready",
        gen: scan.gen,
        request: scan.request,
        scanId,
        summary: event.summary ?? null,
        view: null,
        children: EMPTY_CHILDREN,
        issues: scan.kind === "authorizing" ? [] : scan.liveIssues,
        nav: { history: [], index: -1 },
        partial
      };
    }
    case "failed":
      return {
        kind: "failed",
        gen: scan.gen,
        request: scan.request,
        message: event.message ?? ""
      };
  }
}

export function scanReducer(scan: Scan, action: ScanAction): Scan {
  if (action.type === "start") {
    return action.request.mode === "administrator"
      ? { kind: "authorizing", gen: action.gen, request: action.request, scanId: null }
      : {
          kind: "scanning",
          gen: action.gen,
          request: action.request,
          scanId: null,
          progress: null,
          liveIssues: []
        };
  }
  if (action.type === "reset") return { kind: "idle", gen: action.gen };
  if (action.gen !== scan.gen) return scan;

  switch (action.type) {
    case "assigned":
      return scan.kind === "authorizing" ||
        scan.kind === "scanning" ||
        scan.kind === "cancelling"
        ? { ...scan, scanId: scan.scanId ?? action.scanId }
        : scan;
    case "startFailed":
      return scan.kind === "idle"
        ? scan
        : {
            kind: "failed",
            gen: scan.gen,
            request: scan.request,
            message: action.message
          };
    case "event":
      return onEvent(scan, action.event);
    case "cancel":
      return scan.kind === "authorizing"
        ? {
            kind: "cancelling",
            gen: scan.gen,
            request: scan.request,
            scanId: scan.scanId,
            progress: null,
            liveIssues: []
          }
        : scan.kind === "scanning"
          ? { ...scan, kind: "cancelling" }
          : scan;
    case "loaded":
      return scan.kind === "loading"
        ? {
            kind: "ready",
            gen: scan.gen,
            request: scan.request,
            scanId: scan.scanId,
            summary: scan.summary,
            view: action.view,
            children: action.children,
            issues: action.issues,
            nav: { history: [action.view.id], index: 0 },
            partial: scan.partial
          }
        : scan;
    case "loadFailed":
      return scan.kind === "loading"
        ? {
            kind: "ready",
            gen: scan.gen,
            request: scan.request,
            scanId: scan.scanId,
            summary: scan.summary,
            view: null,
            children: EMPTY_CHILDREN,
            issues: [],
            nav: { history: [], index: -1 },
            partial: scan.partial
          }
        : scan;
    case "view": {
      if (scan.kind !== "ready") return scan;
      let nav = scan.nav;
      if (action.nav === "push") {
        const history = scan.nav.history.slice(0, scan.nav.index + 1);
        history.push(action.view.id);
        nav = { history, index: history.length - 1 };
      } else if (typeof action.nav === "number") {
        if (action.nav < 0 || action.nav >= scan.nav.history.length) return scan;
        nav = { ...scan.nav, index: action.nav };
      }
      return { ...scan, view: action.view, children: action.children, nav };
    }
    case "more":
      // Only the page that continues the current list; repeats and stale folders are dropped.
      return scan.kind === "ready" &&
        scan.view?.id === action.nodeId &&
        scan.children.items.length === action.offset
        ? {
            ...scan,
            children: {
              items: [...scan.children.items, ...action.page.items],
              total: action.page.total
            }
          }
        : scan;
  }
}
