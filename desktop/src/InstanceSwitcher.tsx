import { useCallback, useEffect, useId, useRef, useState } from "react";
import { motion, useReducedMotion, useSpring, useTransform, type MotionValue } from "motion/react";
import { carouselSlot, clampAnchor, curvePose, insertionFrames, INSTANCE_SPRING, INSERT_SECONDS, FADE_SECONDS } from "./instanceMotion";

export interface PickerInstance {
  name: string;
  version: string | null;
  kind: string;
  status: "stopped" | "running" | "installing";
}

interface Props {
  instances: PickerInstance[];
  selected: string;
  open: boolean;
  loading: boolean;
  disabled: boolean;
  onOpenChange: (open: boolean) => void;
  onSelect: (name: string) => void;
  onEmpty: () => void;
}

function CardContents({ instance }: { instance: PickerInstance }) {
  return <>
    <span className="instance-core">{instance.kind}</span>
    <span className="instance-version">{instance.version || "—"}</span>
    <span className="instance-option-detail"><span>{instance.name}</span>
      <span className={`instance-state ${instance.status}`}>{instance.status === "running" ? "运行中" : instance.status === "installing" ? "安装中" : "已停止"}</span>
    </span>
  </>;
}

function RailCard({ instance, index, position, id, active, interactive, choose }: {
  instance: PickerInstance; index: number; position: MotionValue<number>; id: string;
  active: boolean; interactive: boolean; choose: (instance: PickerInstance, slot: number) => void;
}) {
  const x = useTransform(position, value => curvePose(carouselSlot(index, value)).x);
  const y = useTransform(position, value => curvePose(carouselSlot(index, value)).y);
  const scale = useTransform(position, value => curvePose(carouselSlot(index, value)).scale);
  const opacity = useTransform(position, value => curvePose(carouselSlot(index, value)).opacity);
  const visibility = useTransform(position, value => curvePose(carouselSlot(index, value)).opacity > 0.01 ? "visible" as const : "hidden" as const);
  const pointerEvents = useTransform(position, value => Math.abs(carouselSlot(index, value)) > 0.5 && curvePose(carouselSlot(index, value)).opacity > 0.02 ? "auto" as const : "none" as const);
  return <motion.button
    className={`instance-option ${active ? "is-candidate" : ""}`} id={id} type="button" role="option"
    aria-selected={false} aria-label={`${instance.name}，${instance.kind}，${instance.version || "未知版本"}`}
    tabIndex={-1} disabled={!interactive}
    style={{ x, y, scale, opacity, visibility, pointerEvents: interactive ? pointerEvents : "none" }}
    onClick={event => { if (event.button === 0) choose(instance, carouselSlot(index, position.get())); }}
  ><CardContents instance={instance} /></motion.button>;
}

export function InstanceSwitcher({ instances, selected, open, loading, disabled, onOpenChange, onSelect, onEmpty }: Props) {
  const listId = useId();
  const container = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const current = instances.find(instance => instance.name === selected);
  const alternatives = instances.filter(instance => instance.name !== selected);
  const selectedIndex = Math.max(0, instances.findIndex(instance => instance.name === selected));
  const [anchor, setAnchor] = useState(selectedIndex);
  const anchorRef = useRef(anchor);
  const position = useSpring(selectedIndex, INSTANCE_SPRING);
  const reducedMotion = useReducedMotion() ?? false;
  const wasOpen = useRef(false);
  const previousSelected = useRef(selected);
  const pendingStep = useRef(0);
  const wheelState = useRef({ total: 0, direction: 0, lastStep: -Infinity });
  const keyboardMode = useRef(false);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const insertionTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [inserting, setInserting] = useState<{ instance: PickerInstance; old: PickerInstance | undefined; slot: number } | null>(null);
  const insertingRef = useRef(false);
  const candidateIndex = Math.max(0, Math.min(alternatives.length - 1, anchor - 1));
  const candidate = alternatives[candidateIndex];
  const inventoryKey = instances.map(instance => instance.name).join("\0");

  useEffect(() => {
    const newlyOpened = open && !wasOpen.current;
    const selectedChanged = selected !== previousSelected.current;
    if (newlyOpened || selectedChanged) {
      const next = clampAnchor(selectedIndex + (newlyOpened ? pendingStep.current : 0), alternatives.length);
      pendingStep.current = 0;
      anchorRef.current = next;
      setAnchor(next);
      if (newlyOpened || reducedMotion) position.jump(next);
      else position.set(next);
    } else {
      const next = clampAnchor(anchorRef.current, alternatives.length);
      if (next !== anchorRef.current) { anchorRef.current = next; setAnchor(next); }
    }
    wasOpen.current = open;
    previousSelected.current = selected;
  }, [open, selected, inventoryKey, selectedIndex, alternatives.length, position, reducedMotion]);

  useEffect(() => {
    if (reducedMotion) position.jump(anchor);
    else position.set(anchor);
  }, [anchor, position, reducedMotion]);

  const close = useCallback(() => {
    onOpenChange(false);
    pendingStep.current = 0;
    wheelState.current.total = 0;
  }, [onOpenChange]);

  const finishInsertion = useCallback(() => {
    if (!insertingRef.current) return;
    insertingRef.current = false;
    if (insertionTimer.current) clearTimeout(insertionTimer.current);
    setInserting(null);
    close();
    trigger.current?.focus({ preventScroll: true });
  }, [close]);

  useEffect(() => () => {
    if (closeTimer.current) clearTimeout(closeTimer.current);
    if (insertionTimer.current) clearTimeout(insertionTimer.current);
  }, []);

  const advance = useCallback((step: number) => {
    const next = clampAnchor(anchorRef.current + step, alternatives.length);
    anchorRef.current = next;
    setAnchor(next);
  }, [alternatives.length]);

  useEffect(() => {
    const target = container.current;
    if (!target) return;
    const wheel = (event: WheelEvent) => {
      if (disabled || insertingRef.current || !alternatives.length || event.deltaY === 0) return;
      event.preventDefault();
      event.stopPropagation();
      const direction = Math.sign(event.deltaY);
      const now = performance.now();
      const state = wheelState.current;
      if (direction !== state.direction) { state.total = 0; state.direction = direction; state.lastStep = -Infinity; }
      const delta = event.deltaY * (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 110 : 1);
      if (now - state.lastStep < 100) return;
      state.total += delta;
      if (!open) onOpenChange(true);
      if (Math.abs(state.total) < 40) return;
      state.total = 0;
      state.lastStep = now;
      // One detent per wheel impulse; trackpad momentum cannot skip entire pages.
      if (!open) pendingStep.current = direction;
      else advance(direction);
    };
    target.addEventListener("wheel", wheel, { passive: false });
    return () => target.removeEventListener("wheel", wheel);
  }, [disabled, alternatives.length, open, advance, onOpenChange]);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => {
      if (!insertingRef.current && !container.current?.contains(event.target as Node)) close();
    };
    document.addEventListener("pointerdown", dismiss);
    return () => document.removeEventListener("pointerdown", dismiss);
  }, [open, close]);

  const choose = (instance: PickerInstance, slot: number) => {
    if (disabled || insertingRef.current) return;
    if (reducedMotion) { onSelect(instance.name); close(); trigger.current?.focus({ preventScroll: true }); return; }
    insertingRef.current = true;
    setInserting({ instance, old: current, slot });
    onSelect(instance.name);
    // Completion fallback also handles background windows where frames can be throttled.
    insertionTimer.current = setTimeout(finishInsertion, INSERT_SECONDS * 1000 + 80);
  };

  const displayed = inserting ? inserting.old : current;
  const initialPose = inserting ? curvePose(inserting.slot) : null;

  return <div ref={container} className="instance-switcher"
    onPointerEnter={event => {
      if (closeTimer.current) clearTimeout(closeTimer.current);
      keyboardMode.current = false;
      if (event.pointerType !== "touch" && !disabled && alternatives.length && !insertingRef.current) onOpenChange(true);
    }}
    onPointerLeave={() => {
      if (!keyboardMode.current && !insertingRef.current) closeTimer.current = setTimeout(close, 120);
    }}
    onBlur={event => { if (!insertingRef.current && !event.currentTarget.contains(event.relatedTarget)) close(); }}
    onKeyDown={event => {
      keyboardMode.current = true;
      if (disabled || insertingRef.current) return;
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close(); trigger.current?.focus(); return; }
      if (open && ["Enter", " "].includes(event.key) && candidate) {
        event.preventDefault(); choose(candidate, carouselSlot(candidateIndex, position.get())); return;
      }
      if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key) || !alternatives.length) return;
      event.preventDefault();
      if (!open) { onOpenChange(true); return; }
      if (event.key === "Home" || event.key === "End") {
        const next = event.key === "Home" ? 0 : alternatives.length;
        anchorRef.current = next; setAnchor(next);
      } else advance(event.key === "ArrowDown" ? 1 : -1);
    }}
  >
    <motion.div className="instance-popover" initial={false}
      animate={{ opacity: open ? 1 : 0, visibility: open ? "visible" : "hidden" }}
      transition={{ opacity: { duration: reducedMotion ? 0 : FADE_SECONDS }, visibility: { delay: open || reducedMotion ? 0 : FADE_SECONDS } }}
      aria-hidden={!open}
    >
      <div className="instance-rail" data-interactive={open ? "true" : "false"}>
        <div className="instance-hover-bridge" style={{ pointerEvents: open ? "auto" : "none" }} />
      </div>
      <div className="instance-options" id={listId} role="listbox" aria-label="实例列表"
        aria-activedescendant={candidate ? `${listId}-${candidateIndex}` : undefined} data-anchor={anchor}>
        {alternatives.map((instance, index) => <RailCard key={instance.name} instance={instance} index={index} position={position}
          id={`${listId}-${index}`} active={index === candidateIndex} interactive={open && !disabled && !inserting}
          choose={choose} />)}
      </div>
    </motion.div>
    <button ref={trigger} className={`selected-instance ${inserting ? "is-replacing" : ""}`} type="button" disabled={disabled || loading || !!inserting}
      aria-label="选择实例" aria-haspopup="listbox" aria-expanded={open} aria-controls={open ? listId : undefined}
      title="滚轮逐格浏览，左键选择实例" onClick={() => instances.length ? onOpenChange(!open) : onEmpty()}>
      <span className="instance-version">{loading ? "…" : displayed?.version || (displayed?.status === "installing" ? "Installing" : "—")}</span>
      <span className="instance-name">{displayed ? `${displayed.name} · ${displayed.kind}` : "选择或添加实例"}</span>
      {instances.length > 1 && <span className="instance-count" aria-hidden="true">{instances.length} ◂</span>}
    </button>
    {inserting && initialPose && <motion.div key={inserting.instance.name} className="instance-insertion"
      aria-hidden="true" initial={{ x: initialPose.x, y: initialPose.y, scale: initialPose.scale, opacity: 0.95 }}
      animate={insertionFrames(inserting.slot)}
      transition={{ duration: INSERT_SECONDS, times: [0, 0.28, 0.62, 1], ease: [0.22, 0.75, 0.18, 1] }}
      onAnimationComplete={finishInsertion}><CardContents instance={inserting.instance} /></motion.div>}
  </div>;
}
