import { describe, expect, it } from "vitest";
import type { ScanEvent, ScanRequest, ScanSummary, ViewNode } from "../types";
import {
  MAX_LIVE_ISSUES,
  canStop,
  isBusy,
  scanReducer,
  type Scan,
  type ScanAction,
  type ScanKind
} from "./scanMachine";

const request = (mode: ScanRequest["mode"] = "standard"): ScanRequest => ({
  target: "/home/alex",
  mode,
  options: {
    crossFilesystems: false,
    includeRemoteMounts: false,
    includeRemovable: true,
    showSmallFiles: false,
    exclusions: []
  }
});

const summary: ScanSummary = {
  files: 1,
  directories: 1,
  allocatedBytes: 1,
  apparentBytes: 1,
  issues: 0,
  excluded: 0,
  elapsedMs: 1,
  status: "complete",
  elevated: false
};

const view = (id: number): ViewNode => ({
  id,
  parentId: null,
  name: `n${id}`,
  displayPath: `/n${id}`,
  kind: "directory",
  allocatedBytes: 1,
  apparentBytes: 1,
  childCount: 0,
  fileCount: 0,
  directoryCount: 1,
  flags: [],
  children: []
});

const page = (ids: number[], total = 10) => ({
  items: ids.map((id) => view(id)),
  total
});

const GEN = 1;

const states: Record<string, Scan> = {
  idle: { kind: "idle", gen: GEN },
  authorizing: { kind: "authorizing", gen: GEN, request: request("administrator"), scanId: "a" },
  scanning: {
    kind: "scanning",
    gen: GEN,
    request: request(),
    scanId: "a",
    progress: null,
    liveIssues: []
  },
  cancelling: {
    kind: "cancelling",
    gen: GEN,
    request: request(),
    scanId: "a",
    progress: null,
    liveIssues: []
  },
  loading: { kind: "loading", gen: GEN, request: request(), scanId: "a", summary, partial: false },
  ready: {
    kind: "ready",
    gen: GEN,
    request: request(),
    scanId: "a",
    summary,
    view: view(0),
    children: page([1, 2]),
    issues: [],
    nav: { history: [0], index: 0 },
    partial: false
  },
  failed: { kind: "failed", gen: GEN, request: request(), message: "boom" }
};

const event = (value: Omit<ScanEvent, "scanId">): ScanEvent => ({ scanId: "a", ...value });

const actions: Record<string, (gen: number) => ScanAction> = {
  assigned: (gen) => ({ type: "assigned", gen, scanId: "b" }),
  startFailed: (gen) => ({ type: "startFailed", gen, message: "no" }),
  evAuthorizing: (gen) => ({ type: "event", gen, event: event({ event: "authorizing" }) }),
  evStarted: (gen) => ({ type: "event", gen, event: event({ event: "started" }) }),
  evProgress: (gen) => ({
    type: "event",
    gen,
    event: event({
      event: "progress",
      progress: {
        filesScanned: 5,
        directoriesScanned: 1,
        bytesScanned: 9,
        currentPath: "/x",
        issues: 0
      }
    })
  }),
  evIssue: (gen) => ({
    type: "event",
    gen,
    event: event({ event: "issue", issue: { path: "/x", kind: "io", message: "x" } })
  }),
  evCompleted: (gen) => ({ type: "event", gen, event: event({ event: "completed", summary }) }),
  evCancelledWithTree: (gen) => ({
    type: "event",
    gen,
    event: event({ event: "cancelled", summary: { ...summary, status: "cancelled" } })
  }),
  evCancelledEmpty: (gen) => ({ type: "event", gen, event: event({ event: "cancelled" }) }),
  evFailed: (gen) => ({ type: "event", gen, event: event({ event: "failed", message: "x" }) }),
  cancel: (gen) => ({ type: "cancel", gen }),
  loaded: (gen) => ({ type: "loaded", gen, view: view(0), children: page([1]), issues: [] }),
  loadFailed: (gen) => ({ type: "loadFailed", gen }),
  view: (gen) => ({ type: "view", gen, view: view(5), children: page([]), nav: "push" }),
  more: (gen) => ({ type: "more", gen, nodeId: 0, offset: 2, page: page([3], 10) })
};

// Expected next kind for every (state, action) pair under the current gen.
const table: Record<string, Record<string, ScanKind>> = {
  idle: Object.fromEntries(Object.keys(actions).map((name) => [name, "idle"])),
  authorizing: {
    assigned: "authorizing",
    startFailed: "failed",
    evAuthorizing: "authorizing",
    evStarted: "scanning",
    evProgress: "authorizing",
    evIssue: "authorizing",
    evCompleted: "loading",
    evCancelledWithTree: "loading",
    evCancelledEmpty: "ready",
    evFailed: "failed",
    cancel: "cancelling",
    loaded: "authorizing",
    loadFailed: "authorizing",
    view: "authorizing",
    more: "authorizing"
  },
  scanning: {
    assigned: "scanning",
    startFailed: "failed",
    evAuthorizing: "scanning",
    evStarted: "scanning",
    evProgress: "scanning",
    evIssue: "scanning",
    evCompleted: "loading",
    evCancelledWithTree: "loading",
    evCancelledEmpty: "ready",
    evFailed: "failed",
    cancel: "cancelling",
    loaded: "scanning",
    loadFailed: "scanning",
    view: "scanning",
    more: "scanning"
  },
  cancelling: {
    assigned: "cancelling",
    startFailed: "failed",
    evAuthorizing: "cancelling",
    evStarted: "cancelling",
    evProgress: "cancelling",
    evIssue: "cancelling",
    evCompleted: "loading",
    evCancelledWithTree: "loading",
    evCancelledEmpty: "ready",
    evFailed: "failed",
    cancel: "cancelling",
    loaded: "cancelling",
    loadFailed: "cancelling",
    view: "cancelling",
    more: "cancelling"
  },
  loading: {
    ...Object.fromEntries(Object.keys(actions).map((name) => [name, "loading"])),
    startFailed: "failed",
    loaded: "ready",
    loadFailed: "ready"
  },
  ready: {
    ...Object.fromEntries(Object.keys(actions).map((name) => [name, "ready"])),
    startFailed: "failed"
  },
  failed: Object.fromEntries(Object.keys(actions).map((name) => [name, "failed"]))
};

describe("scanReducer transition table", () => {
  for (const [stateName, state] of Object.entries(states)) {
    it(`covers every action from ${stateName}`, () => {
      expect(Object.keys(table[stateName]).sort()).toEqual(Object.keys(actions).sort());
      for (const [actionName, make] of Object.entries(actions)) {
        const next = scanReducer(state, make(GEN));
        expect(`${actionName} -> ${next.kind}`).toBe(
          `${actionName} -> ${table[stateName][actionName]}`
        );
        expect(next.gen).toBe(GEN);
      }
    });

    it(`drops every stale action in ${stateName}`, () => {
      for (const make of Object.values(actions)) {
        expect(scanReducer(state, make(GEN - 1))).toBe(state);
        expect(scanReducer(state, make(GEN + 1))).toBe(state);
      }
    });

    it(`start and reset leave ${stateName} for a new gen`, () => {
      expect(scanReducer(state, { type: "start", gen: 7, request: request() })).toMatchObject({
        kind: "scanning",
        gen: 7,
        scanId: null
      });
      expect(
        scanReducer(state, { type: "start", gen: 7, request: request("administrator") })
      ).toMatchObject({ kind: "authorizing", gen: 7 });
      expect(scanReducer(state, { type: "reset", gen: 8 })).toEqual({ kind: "idle", gen: 8 });
    });
  }
});

describe("scanReducer details", () => {
  it("keeps the first scan id and adopts it from events", () => {
    const started = scanReducer(
      { ...states.scanning, scanId: null } as Scan,
      actions.evProgress(GEN)
    );
    expect(started).toMatchObject({ scanId: "a", progress: { filesScanned: 5 } });
    expect(scanReducer(started, actions.assigned(GEN))).toMatchObject({ scanId: "a" });
  });

  it("cancels before the backend id is known", () => {
    const authorizing: Scan = { ...states.authorizing, scanId: null } as Scan;
    const cancelling = scanReducer(authorizing, actions.cancel(GEN));
    expect(cancelling).toMatchObject({ kind: "cancelling", scanId: null });
    expect(scanReducer(cancelling, actions.assigned(GEN))).toMatchObject({ scanId: "b" });
  });

  it("marks cancelled results as partial and finished ones as complete", () => {
    expect(scanReducer(states.scanning, actions.evCancelledWithTree(GEN))).toMatchObject({
      kind: "loading",
      partial: true
    });
    expect(scanReducer(states.scanning, actions.evCompleted(GEN))).toMatchObject({
      kind: "loading",
      partial: false
    });
    expect(scanReducer(states.cancelling, actions.evCancelledEmpty(GEN))).toMatchObject({
      kind: "ready",
      view: null,
      partial: true
    });
  });

  it("keeps the failure message", () => {
    expect(scanReducer(states.scanning, actions.evFailed(GEN))).toMatchObject({
      kind: "failed",
      message: "x"
    });
  });

  it("caps live issues", () => {
    let scan: Scan = {
      ...(states.scanning as Extract<Scan, { kind: "scanning" | "cancelling" }>),
      liveIssues: Array.from({ length: MAX_LIVE_ISSUES }, () => ({
        path: "/x",
        kind: "io" as const,
        message: ""
      }))
    };
    scan = scanReducer(scan, actions.evIssue(GEN));
    expect(scan.kind === "scanning" && scan.liveIssues.length).toBe(MAX_LIVE_ISSUES);
  });

  it("starts history at the loaded root", () => {
    expect(scanReducer(states.loading, actions.loaded(GEN))).toMatchObject({
      kind: "ready",
      nav: { history: [0], index: 0 },
      children: { total: 10 }
    });
    expect(scanReducer(states.loading, actions.loadFailed(GEN))).toMatchObject({
      kind: "ready",
      view: null,
      partial: false
    });
  });

  it("pushes, truncates, moves and replaces history from the current state", () => {
    const push = (scan: Scan, id: number) =>
      scanReducer(scan, { type: "view", gen: GEN, view: view(id), children: page([]), nav: "push" });
    let scan = push(push(states.ready, 1), 2);
    expect(scan).toMatchObject({ nav: { history: [0, 1, 2], index: 2 } });
    scan = scanReducer(scan, { type: "view", gen: GEN, view: view(0), children: page([]), nav: 0 });
    expect(scan).toMatchObject({ nav: { history: [0, 1, 2], index: 0 } });
    scan = push(scan, 9);
    expect(scan).toMatchObject({ nav: { history: [0, 9], index: 1 } });
    scan = scanReducer(scan, {
      type: "view",
      gen: GEN,
      view: view(9),
      children: page([4]),
      nav: "replace"
    });
    expect(scan).toMatchObject({ nav: { history: [0, 9], index: 1 }, children: { items: [{ id: 4 }] } });
    expect(
      scanReducer(scan, { type: "view", gen: GEN, view: view(1), children: page([]), nav: 5 })
    ).toBe(scan);
  });

  it("appends only the next page of the shown folder", () => {
    const more = scanReducer(states.ready, actions.more(GEN));
    expect(more.kind === "ready" && more.children.items.map((item) => item.id)).toEqual([
      1, 2, 3
    ]);
    expect(scanReducer(more, actions.more(GEN))).toBe(more);
    expect(
      scanReducer(states.ready, { type: "more", gen: GEN, nodeId: 4, offset: 2, page: page([3]) })
    ).toBe(states.ready);
  });

  it("reports busy and stoppable states", () => {
    const busy = Object.values(states).filter(isBusy).map((scan) => scan.kind);
    expect(busy).toEqual(["authorizing", "scanning", "cancelling", "loading"]);
    const stoppable = Object.values(states).filter(canStop).map((scan) => scan.kind);
    expect(stoppable).toEqual(["authorizing", "scanning"]);
  });
});
