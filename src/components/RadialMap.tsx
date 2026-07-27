import {
  hierarchy,
  partition,
  type HierarchyRectangularNode
} from "d3-hierarchy";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState
} from "react";
import type { ByteUnitScale, ColorScheme, ViewNode } from "../types";
import { formatBytes } from "../lib/format";
import { useI18n } from "../lib/i18n";

interface RadialMapProps {
  root: ViewNode;
  scheme: ColorScheme;
  contrast: number;
  byteUnitScale: ByteUnitScale;
  selectedId: number | null;
  onSelect: (node: ViewNode) => void;
  onCenter: (node: ViewNode) => void;
}

interface HitArc {
  node: HierarchyRectangularNode<ViewNode>;
  inner: number;
  outer: number;
}

const SYSTEM_COLORS = [
  "#0b7955",
  "#1c8b68",
  "#389879",
  "#4d866f",
  "#6f8e80",
  "#547a68",
  "#2e6e5a"
];
const RAINBOW_COLORS = [
  "#c55a3d",
  "#d28c33",
  "#8c9c43",
  "#2f8c73",
  "#327da8",
  "#6f69ad",
  "#a3537e"
];
const HIGH_CONTRAST_COLORS = [
  "#006b4f",
  "#9a4b00",
  "#235cb2",
  "#7a3fb0",
  "#a4243b",
  "#496b00",
  "#006d7d"
];

function palette(scheme: ColorScheme) {
  if (scheme === "rainbow") return RAINBOW_COLORS;
  if (scheme === "high_contrast") return HIGH_CONTRAST_COLORS;
  return SYSTEM_COLORS;
}

function topBranch(node: HierarchyRectangularNode<ViewNode>) {
  let current = node;
  while (current.parent && current.parent.parent) current = current.parent;
  return current;
}

function colorFor(
  node: HierarchyRectangularNode<ViewNode>,
  scheme: ColorScheme,
  contrast: number,
  selected: boolean
) {
  const colors = palette(scheme);
  const branch = topBranch(node);
  const siblings = branch.parent?.children ?? [branch];
  const index = Math.max(0, siblings.indexOf(branch));
  const base = colors[index % colors.length];
  const depthLightness = Math.min(0.32, (node.depth - 1) * 0.07);
  const amount = depthLightness * (0.55 + contrast / 180);
  return mixWithSurface(base, amount, selected);
}

function mixWithSurface(hex: string, amount: number, selected: boolean) {
  const dark = document.documentElement.dataset.theme === "dark";
  const target = dark ? [32, 38, 35] : [238, 242, 240];
  const value = hex.replace("#", "");
  const source = [
    Number.parseInt(value.slice(0, 2), 16),
    Number.parseInt(value.slice(2, 4), 16),
    Number.parseInt(value.slice(4, 6), 16)
  ];
  const blend = selected ? Math.max(0, amount - 0.13) : amount;
  const mixed = source.map((channel, index) =>
    Math.round(channel + (target[index] - channel) * blend)
  );
  return `rgb(${mixed.join(",")})`;
}

export function RadialMap({
  root,
  scheme,
  contrast,
  byteUnitScale,
  selectedId,
  onSelect,
  onCenter
}: RadialMapProps) {
  const { locale, t } = useI18n();
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const wrapRef = useRef<HTMLDivElement>(null);
  const arcsRef = useRef<HitArc[]>([]);
  const [size, setSize] = useState({ width: 640, height: 640 });
  const [hovered, setHovered] = useState<{
    node: ViewNode;
    x: number;
    y: number;
  } | null>(null);

  const layout = useMemo(() => {
    const data = hierarchy(root, (node) => node.children)
      .sum((node) =>
        node.children.length === 0 ? Math.max(1, node.allocatedBytes) : 0
      )
      .sort((left, right) => (right.value ?? 0) - (left.value ?? 0));
    return partition<ViewNode>().size([Math.PI * 2, 1])(data);
  }, [root]);

  useEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap) return;
    const observer = new ResizeObserver(([entry]) => {
      const width = Math.max(280, entry.contentRect.width);
      const height = Math.max(280, entry.contentRect.height);
      setSize({ width, height });
    });
    observer.observe(wrap);
    return () => observer.disconnect();
  }, []);

  const paint = useCallback(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ratio = window.devicePixelRatio || 1;
    canvas.width = Math.floor(size.width * ratio);
    canvas.height = Math.floor(size.height * ratio);
    canvas.style.width = `${size.width}px`;
    canvas.style.height = `${size.height}px`;
    const context = canvas.getContext("2d");
    if (!context) return;
    context.scale(ratio, ratio);

    const styles = getComputedStyle(document.documentElement);
    const surface = styles.getPropertyValue("--map-surface").trim() || "#f2f5f3";
    const text = styles.getPropertyValue("--text-primary").trim() || "#1e2522";
    const muted = styles.getPropertyValue("--text-muted").trim() || "#68716d";
    const border = styles.getPropertyValue("--map-border").trim() || "#d8dfdb";
    context.clearRect(0, 0, size.width, size.height);

    const cx = size.width / 2;
    const cy = size.height / 2;
    const radius = Math.max(100, Math.min(size.width, size.height) * 0.43);
    const hole = Math.max(58, radius * 0.19);
    const maxDepth = Math.max(1, ...layout.descendants().map((node) => node.depth));
    const ring = (radius - hole) / maxDepth;
    const hitArcs: HitArc[] = [];

    context.save();
    context.translate(cx, cy);
    for (const node of layout.descendants()) {
      if (node.depth === 0 || node.x1 - node.x0 < 0.0002) continue;
      const inner = hole + (node.depth - 1) * ring + 1.5;
      const outer = hole + node.depth * ring - 1.5;
      const start = node.x0 - Math.PI / 2;
      const end = node.x1 - Math.PI / 2;
      hitArcs.push({ node, inner, outer });

      context.beginPath();
      context.arc(0, 0, outer, start, end);
      context.arc(0, 0, inner, end, start, true);
      context.closePath();
      context.fillStyle = colorFor(
        node,
        scheme,
        contrast,
        node.data.id === selectedId
      );
      context.fill();
      context.lineWidth = node.data.id === selectedId ? 2.5 : 1;
      context.strokeStyle =
        node.data.id === selectedId
          ? styles.getPropertyValue("--focus-ring").trim() || "#075f44"
          : surface;
      context.stroke();

      const angle = end - start;
      const middleRadius = (inner + outer) / 2;
      const arcLength = angle * middleRadius;
      if (arcLength > 56 && outer - inner > 24) {
        const centerAngle = (start + end) / 2;
        const label =
          node.data.kind === "small_files"
            ? t("nodeSmallFiles")
            : node.data.name;
        const maxWidth = Math.max(30, arcLength - 16);
        context.save();
        context.rotate(centerAngle);
        context.translate(middleRadius, 0);
        if (centerAngle > Math.PI / 2 && centerAngle < (Math.PI * 3) / 2) {
          context.rotate(Math.PI);
        }
        context.font = "600 11px system-ui, sans-serif";
        context.fillStyle = "#f8faf9";
        context.textAlign = "center";
        context.textBaseline = "middle";
        let output = label;
        while (
          output.length > 4 &&
          context.measureText(output).width > maxWidth
        ) {
          output = `${output.slice(0, -2)}…`;
        }
        if (context.measureText(output).width <= maxWidth) {
          context.fillText(output, 0, 0);
        }
        context.restore();
      }
    }

    context.beginPath();
    context.arc(0, 0, hole - 5, 0, Math.PI * 2);
    context.fillStyle = surface;
    context.fill();
    context.strokeStyle = border;
    context.lineWidth = 1;
    context.stroke();

    const centerName =
      root.name.length > 16 ? `${root.name.slice(0, 14)}…` : root.name;
    context.fillStyle = text;
    context.font = "700 13px system-ui, sans-serif";
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText(centerName, 0, -7);
    context.fillStyle = muted;
    context.font = "500 10px system-ui, sans-serif";
    context.fillText(
      formatBytes(root.allocatedBytes, byteUnitScale, locale),
      0,
      11
    );
    context.restore();
    arcsRef.current = hitArcs;
  }, [
    byteUnitScale,
    contrast,
    layout,
    locale,
    root,
    scheme,
    selectedId,
    size,
    t
  ]);

  useEffect(() => {
    paint();
  }, [paint]);

  function hitTest(
    event:
      | React.PointerEvent<HTMLCanvasElement>
      | React.MouseEvent<HTMLCanvasElement>
  ) {
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = event.clientX - bounds.left - bounds.width / 2;
    const y = event.clientY - bounds.top - bounds.height / 2;
    const distance = Math.sqrt(x * x + y * y);
    let angle = Math.atan2(y, x) + Math.PI / 2;
    if (angle < 0) angle += Math.PI * 2;
    const hit = [...arcsRef.current]
      .reverse()
      .find(
        (arc) =>
          distance >= arc.inner &&
          distance <= arc.outer &&
          angle >= arc.node.x0 &&
          angle <= arc.node.x1
      );
    return hit?.node.data ?? null;
  }

  return (
    <div className="radial-map" ref={wrapRef}>
      <canvas
        ref={canvasRef}
        aria-label={t("diskMapAria", { path: root.displayPath })}
        role="img"
        onPointerMove={(event) => {
          const node = hitTest(event);
          setHovered(
            node
              ? {
                  node,
                  x: event.clientX,
                  y: event.clientY
                }
              : null
          );
        }}
        onPointerLeave={() => setHovered(null)}
        onClick={(event) => {
          const node = hitTest(event);
          if (!node) return;
          if (node.kind === "directory") onCenter(node);
          else if (node.kind !== "small_files") onSelect(node);
        }}
        onContextMenu={(event) => {
          event.preventDefault();
          const node = hitTest(event);
          if (node && node.kind !== "small_files") onSelect(node);
        }}
      />
      {hovered && (
        <div
          className="map-tooltip"
          style={{
            left: Math.min(hovered.x + 14, window.innerWidth - 230),
            top: Math.min(hovered.y + 14, window.innerHeight - 110)
          }}
        >
          <strong>
            {hovered.node.kind === "small_files"
              ? t("nodeSmallFiles")
              : hovered.node.name}
          </strong>
          <span>
            {formatBytes(
              hovered.node.allocatedBytes,
              byteUnitScale,
              locale
            )}
          </span>
          <small dir="auto">{hovered.node.displayPath}</small>
        </div>
      )}
    </div>
  );
}
