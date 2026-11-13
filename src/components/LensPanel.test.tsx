import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ReclaimMatch, ScanDiff } from "../types";
import { I18nProvider } from "../lib/i18n";
import { ComparePanel, ReclaimPanel } from "./LensPanel";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(() => Promise.resolve())
}));

const diff: ScanDiff = {
  beforeTimestamp: 1_700_000_000,
  beforeTotal: 1000,
  afterTotal: 1400,
  delta: 400,
  rows: [
    { path: "/h/new", before: 0, after: 500, delta: 500, nodeId: 4 },
    { path: "/h/old", before: 300, after: 0, delta: -300, nodeId: null },
    { path: "/h/same", before: 5, after: 5, delta: 0, nodeId: 2 }
  ]
};

const match = (over: Partial<ReclaimMatch>): ReclaimMatch => ({
  ruleId: "pacman_cache",
  titleKey: "reclaimPacmanTitle",
  explanationKey: "reclaimPackageCacheExplanation",
  command: "sudo paccache -rk2",
  warning: false,
  counted: true,
  path: "/var/cache/pacman/pkg",
  sizeBytes: 2048,
  nodeId: 9,
  ...over
});

afterEach(cleanup);

describe("ComparePanel", () => {
  it("lists growth and shrinkage with signs and navigates on click", () => {
    const onShow = vi.fn();
    render(
      <I18nProvider locale="en">
        <ComparePanel diff={diff} scale="binary" onShow={onShow} onExit={() => undefined} />
      </I18nProvider>
    );
    expect(screen.getByText("+500 B")).toBeInTheDocument();
    expect(screen.getByText("−300 B")).toBeInTheDocument();
    expect(screen.getByText("+400 B")).toBeInTheDocument();
    expect(screen.queryByText("/h/same")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /\/h\/new/ }));
    expect(onShow).toHaveBeenCalledWith(4);
    // A folder that no longer exists cannot be shown on the map.
    expect(screen.getByRole("button", { name: /\/h\/old/ })).toBeDisabled();
  });

  it("offers a way out", () => {
    const onExit = vi.fn();
    render(
      <I18nProvider locale="en">
        <ComparePanel diff={diff} scale="binary" onShow={() => undefined} onExit={onExit} />
      </I18nProvider>
    );
    const exits = screen.getAllByRole("button", { name: "Leave comparison" });
    expect(exits).toHaveLength(2); // header close and the labelled button
    fireEvent.click(exits[1]);
    expect(onExit).toHaveBeenCalled();
  });
});

describe("ReclaimPanel", () => {
  const noop = () => undefined;

  it("shows the total, the command and a copy button", async () => {
    const { writeText } = await import("@tauri-apps/plugin-clipboard-manager");
    const onNotify = vi.fn();
    render(
      <I18nProvider locale="en">
        <ReclaimPanel
          data={{ totalBytes: 2048, matches: [match({})], more: 0 }}
          scale="binary"
          onShow={noop}
          onExit={noop}
          onNotify={onNotify}
        />
      </I18nProvider>
    );
    expect(screen.getByText("Up to 2.00 KiB could be reclaimed")).toBeInTheDocument();
    expect(screen.getByText("Pacman package cache")).toBeInTheDocument();
    expect(screen.getByText("sudo paccache -rk2")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Copy" }));
    expect(writeText).toHaveBeenCalledWith("sudo paccache -rk2");
  });

  it("warns about destructive commands and omits the box when there is no command", () => {
    render(
      <I18nProvider locale="en">
        <ReclaimPanel
          data={{
            totalBytes: 3,
            more: 0,
            matches: [
              match({ ruleId: "coredumps", warning: true, titleKey: "reclaimCoredumpTitle", command: "sudo rm /var/lib/systemd/coredump/*" }),
              match({ ruleId: "user_cache", path: "/h/.cache", command: null, titleKey: "reclaimUserCacheTitle", nodeId: 1 })
            ]
          }}
          scale="binary"
          onShow={noop}
          onExit={noop}
          onNotify={noop}
        />
      </I18nProvider>
    );
    expect(screen.getByText(/Check this command carefully/)).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Copy" })).toHaveLength(1);
  });

  it("explains uncounted items and shows how many more there are", () => {
    render(
      <I18nProvider locale="en">
        <ReclaimPanel
          data={{
            totalBytes: 10,
            more: 7,
            matches: [match({ ruleId: "docker", counted: false, titleKey: "reclaimDockerTitle", command: "docker system prune" })]
          }}
          scale="binary"
          onShow={noop}
          onExit={noop}
          onNotify={noop}
        />
      </I18nProvider>
    );
    expect(screen.getByText(/Not included in this total/)).toBeInTheDocument();
    expect(screen.getByText("7 more items")).toBeInTheDocument();
  });

  it("says so plainly when nothing is reclaimable", () => {
    render(
      <I18nProvider locale="en">
        <ReclaimPanel data={{ totalBytes: 0, matches: [], more: 0 }} scale="binary" onShow={noop} onExit={noop} onNotify={noop} />
      </I18nProvider>
    );
    expect(screen.getByText("Nothing reclaimable found in this scan")).toBeInTheDocument();
  });
});
