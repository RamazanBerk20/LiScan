import { Channel, invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import type {
  NodeDetail,
  NodeSummary,
  ScanEvent,
  ScanRequest,
  Settings,
  ViewNode,
  VolumeInfo
} from "../types";

const isTauri = () => "__TAURI_INTERNALS__" in window;

const demoTree: ViewNode = {
  id: 0,
  parentId: null,
  name: "Home",
  displayPath: "/home/alex",
  kind: "directory",
  allocatedBytes: 189_459_824_640,
  apparentBytes: 191_402_311_680,
  childCount: 6,
  fileCount: 428_741,
  directoryCount: 24_912,
  flags: [],
  children: [
    {
      id: 1,
      parentId: 0,
      name: "Videos",
      displayPath: "/home/alex/Videos",
      kind: "directory",
      allocatedBytes: 74_112_778_240,
      apparentBytes: 74_112_778_240,
      childCount: 3,
      fileCount: 148,
      directoryCount: 12,
      flags: [],
      children: [
        {
          id: 7,
          parentId: 1,
          name: "Projects",
          displayPath: "/home/alex/Videos/Projects",
          kind: "directory",
          allocatedBytes: 42_771_341_312,
          apparentBytes: 42_771_341_312,
          childCount: 0,
          fileCount: 62,
          directoryCount: 4,
          flags: [],
          children: []
        },
        {
          id: 8,
          parentId: 1,
          name: "Recordings",
          displayPath: "/home/alex/Videos/Recordings",
          kind: "directory",
          allocatedBytes: 24_511_389_696,
          apparentBytes: 24_511_389_696,
          childCount: 0,
          fileCount: 71,
          directoryCount: 5,
          flags: [],
          children: []
        },
        {
          id: 9,
          parentId: 1,
          name: "Exports",
          displayPath: "/home/alex/Videos/Exports",
          kind: "directory",
          allocatedBytes: 6_830_047_232,
          apparentBytes: 6_830_047_232,
          childCount: 0,
          fileCount: 15,
          directoryCount: 3,
          flags: [],
          children: []
        }
      ]
    },
    {
      id: 2,
      parentId: 0,
      name: "Projects",
      displayPath: "/home/alex/Projects",
      kind: "directory",
      allocatedBytes: 48_130_588_672,
      apparentBytes: 49_890_588_672,
      childCount: 0,
      fileCount: 180_243,
      directoryCount: 12_482,
      flags: [],
      children: []
    },
    {
      id: 3,
      parentId: 0,
      name: "Downloads",
      displayPath: "/home/alex/Downloads",
      kind: "directory",
      allocatedBytes: 31_980_212_224,
      apparentBytes: 31_980_212_224,
      childCount: 0,
      fileCount: 1_442,
      directoryCount: 238,
      flags: [],
      children: []
    },
    {
      id: 4,
      parentId: 0,
      name: ".cache",
      displayPath: "/home/alex/.cache",
      kind: "directory",
      allocatedBytes: 17_420_713_984,
      apparentBytes: 17_420_713_984,
      childCount: 0,
      fileCount: 212_903,
      directoryCount: 8_041,
      flags: ["hidden"],
      children: []
    },
    {
      id: 5,
      parentId: 0,
      name: "Documents",
      displayPath: "/home/alex/Documents",
      kind: "directory",
      allocatedBytes: 11_742_609_408,
      apparentBytes: 11_925_004_288,
      childCount: 0,
      fileCount: 33_812,
      directoryCount: 3_994,
      flags: [],
      children: []
    },
    {
      id: 6,
      parentId: 0,
      name: "Small files",
      displayPath: "/home/alex",
      kind: "small_files",
      allocatedBytes: 6_073_922_112,
      apparentBytes: 6_073_922_112,
      childCount: 0,
      fileCount: 172,
      directoryCount: 145,
      flags: ["grouped"],
      children: []
    }
  ]
};

const defaultSettings: Settings = {
  theme: "system",
  colorScheme: "system",
  byteUnitScale: "binary",
  contrast: 72,
  showSidebar: true,
  scanOptions: {
    crossFilesystems: false,
    includeRemoteMounts: false,
    includeRemovable: true,
    showSmallFiles: false,
    exclusions: ["/proc", "/sys", "/dev", "/run"]
  }
};

export async function chooseFolder(): Promise<string | null> {
  if (!isTauri()) return "/home/alex";
  const result = await open({ directory: true, multiple: false });
  return typeof result === "string" ? result : null;
}

export async function getSettings(): Promise<Settings> {
  if (!isTauri()) return defaultSettings;
  return invoke<Settings>("get_settings");
}

export async function saveSettings(settings: Settings): Promise<void> {
  if (!isTauri()) return;
  await invoke("update_settings", { settings });
}

export async function listVolumes(): Promise<VolumeInfo[]> {
  if (!isTauri()) {
    return [
      {
        name: "System",
        mountPoint: "/",
        fileSystem: "btrfs",
        totalBytes: 1_024_209_543_168,
        availableBytes: 401_309_949_952,
        removable: false,
        remote: false
      },
      {
        name: "Archive",
        mountPoint: "/mnt/archive",
        fileSystem: "ext4",
        totalBytes: 2_000_398_934_016,
        availableBytes: 1_371_592_097_792,
        removable: false,
        remote: false
      }
    ];
  }
  return invoke<VolumeInfo[]>("list_volumes");
}

export async function getHomePath(): Promise<string> {
  if (!isTauri()) return "/home/alex";
  return invoke<string>("get_home_path");
}

export async function getLaunchRequest(): Promise<ScanRequest | null> {
  if (!isTauri()) return null;
  return invoke<ScanRequest | null>("get_launch_request");
}

export async function startScan(
  request: ScanRequest,
  onEvent: (event: ScanEvent) => void
): Promise<string> {
  if (!isTauri()) {
    const id = crypto.randomUUID();
    onEvent({ event: "started", scanId: id });
    window.setTimeout(
      () =>
        onEvent({
          event: "progress",
          scanId: id,
          progress: {
            filesScanned: 184_203,
            directoriesScanned: 13_891,
            bytesScanned: 142_380_441_600,
            currentPath: `${request.target}/Projects`,
            issues: 0
          }
        }),
      250
    );
    window.setTimeout(
      () =>
        onEvent({
          event: "completed",
          scanId: id,
          summary: {
            files: demoTree.fileCount,
            directories: demoTree.directoryCount,
            allocatedBytes: demoTree.allocatedBytes,
            apparentBytes: demoTree.apparentBytes,
            issues: 0,
            excluded: 4,
            elapsedMs: 1842,
            status: "complete",
            elevated: request.mode === "administrator"
          }
        }),
      650
    );
    return id;
  }

  const channel = new Channel<ScanEvent>();
  channel.onmessage = onEvent;
  return invoke<string>("start_scan", { request, onEvent: channel });
}

export async function cancelScan(scanId: string): Promise<void> {
  if (!isTauri()) return;
  await invoke("cancel_scan", { scanId });
}

export async function getView(
  scanId: string,
  nodeId?: number,
  depth = 4
): Promise<ViewNode> {
  if (!isTauri()) return demoTree;
  return invoke<ViewNode>("get_view", {
    scanId,
    nodeId: nodeId ?? null,
    depth
  });
}

export async function getChildren(
  scanId: string,
  nodeId?: number
): Promise<NodeSummary[]> {
  if (!isTauri()) return demoTree.children;
  return invoke<NodeSummary[]>("get_children", {
    scanId,
    nodeId: nodeId ?? null
  });
}

export async function getNode(
  scanId: string,
  nodeId: number
): Promise<NodeDetail> {
  if (!isTauri()) {
    const node = [demoTree, ...demoTree.children].find((item) => item.id === nodeId);
    if (!node) throw new Error("Node not found");
    return {
      ...node,
      uri: `file://${node.displayPath}`,
      modifiedMs: Date.now() - 86400000,
      permissions: "rwxr-xr-x",
      hardLinks: 1
    };
  }
  return invoke<NodeDetail>("get_node", { scanId, nodeId });
}

export async function getScanIssues(scanId: string): Promise<import("../types").ScanIssue[]> {
  if (!isTauri()) return [];
  return invoke("get_scan_issues", { scanId });
}

export async function performFileAction(
  scanId: string,
  nodeId: number,
  action: string
): Promise<string | null> {
  if (!isTauri()) return null;
  return invoke<string | null>("perform_file_action", {
    scanId,
    nodeId,
    action
  });
}
