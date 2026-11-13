import { describe, expect, it } from "vitest";
import { dropTarget } from "./dropTarget";

describe("dropTarget", () => {
  it("scans local file URLs, decoding names", () => {
    expect(dropTarget("file:///home/alex/100%25%20done\r\n")).toEqual({
      scan: "/home/alex/100% done"
    });
    expect(dropTarget("# comment\nfile://localhost/srv")).toEqual({ scan: "/srv" });
  });

  it("keeps non-UTF-8 names as a file URL for the backend", () => {
    expect(dropTarget("file:///tmp/caf%E9")).toEqual({ scan: "file:///tmp/caf%E9" });
  });

  it("refuses other hosts and junk", () => {
    expect(dropTarget("file://server/share")).toBeNull();
    expect(dropTarget("100%.txt")).toBeNull();
    expect(dropTarget("")).toBeNull();
  });

  it("sends network locations to the remote dialog instead of scanning", () => {
    expect(dropTarget("sftp://host/home/me")).toEqual({ remote: "sftp://host/home/me" });
  });
});
