import * as Tooltip from "@radix-ui/react-tooltip";
import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import type { ScanRequest, ScanSummary, Settings } from "../types";
import { I18nProvider } from "../lib/i18n";
import type { Scan } from "../lib/scanMachine";
import { ScanWorkspace } from "./ScanWorkspace";

const request: ScanRequest = {
  target: "/home/alex",
  mode: "standard",
  options: {
    crossFilesystems: false,
    includeRemoteMounts: false,
    includeRemovable: true,
    showSmallFiles: false,
    exclusions: []
  }
};

const summary: ScanSummary = {
  files: 0,
  directories: 0,
  allocatedBytes: 0,
  apparentBytes: 0,
  issues: 0,
  excluded: 0,
  elapsedMs: 0,
  status: "cancelled",
  elevated: false
};

const settings: Settings = {
  language: "en",
  theme: "system",
  colorScheme: "system",
  byteUnitScale: "binary",
  contrast: 72,
  showSidebar: true,
  keepHistory: true,
  scanOptions: request.options
};

const empty = { items: [], total: 0 };
const nav = { history: [], index: -1 };

const cases: Array<[Exclude<Scan, { kind: "idle" }>, string]> = [
  [{ kind: "authorizing", gen: 1, request, scanId: null }, "Authentication required"],
  [
    { kind: "scanning", gen: 1, request, scanId: "a", progress: null, liveIssues: [] },
    "Reading folders"
  ],
  [
    { kind: "cancelling", gen: 1, request, scanId: "a", progress: null, liveIssues: [] },
    "Cancelling…"
  ],
  [
    { kind: "loading", gen: 1, request, scanId: "a", summary, partial: true },
    "Preparing the map…"
  ],
  [
    {
      kind: "ready",
      gen: 1,
      request,
      scanId: "a",
      summary: null,
      view: null,
      children: empty,
      issues: [],
      nav,
      partial: true
    },
    "Nothing to show"
  ],
  [{ kind: "failed", gen: 1, request, message: "remote scan failed: sftp://x: not mounted" }, "LiScan could not complete this scan"]
];

describe("ScanWorkspace", () => {
  afterEach(cleanup);

  it.each(cases)("renders a distinct screen for $kind", (scan, heading) => {
    const noop = () => undefined;
    render(
      <I18nProvider locale="en">
        <Tooltip.Provider>
          <ScanWorkspace
            scan={scan}
            selectedId={null}
            settings={settings}
            volume={null}
            viewDepth={4}
            onOverview={noop}
            onCancel={noop}
            onRescan={noop}
            onCenter={noop}
            onSelect={noop}
            onBack={noop}
            onForward={noop}
            onUp={noop}
            onSettings={noop}
            onCoverage={noop}
            onReload={noop}
            onMoreChildren={noop}
            onRequestDelete={noop}
            onViewDepth={noop}
            onNotify={noop}
          />
        </Tooltip.Provider>
      </I18nProvider>
    );
    expect(screen.getByRole("heading", { level: 1 })).toHaveTextContent(heading);
    if (scan.kind === "failed") expect(screen.getByText(scan.message)).toBeInTheDocument();
    if (scan.kind === "ready") {
      expect(screen.getByText("The scan was stopped before any folder could be read.")).toBeInTheDocument();
    }
  });
});
