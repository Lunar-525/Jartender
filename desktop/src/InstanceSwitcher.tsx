import { useCallback, useEffect, useId, useRef, useState } from "react";
import {
  animate,
  motion,
  useMotionValue,
  useReducedMotion,
  useTransform,
  type MotionValue,
} from "motion/react";
import {
  carouselSlot,
  clampAnchor,
  curvePose,
  INSTANCE_SPRING,
  ALIGN_SECONDS,
  FADE_SECONDS,
} from "./instanceMotion";
import { InstanceLogo } from "./InstanceLogo";

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
  return (
    <>
      {/* Painted first: the oversized badge sits behind the version and label. */}
      <InstanceLogo kind={instance.kind} />
      <span className="instance-core">{instance.kind}</span>
      <span className="instance-version">{instance.version || "—"}</span>
      <span className="instance-option-detail">
        <span>{instance.name}</span>
        <span className={`instance-state ${instance.status}`}>
          {instance.status === "running"
            ? "运行中"
            : instance.status === "installing"
              ? "安装中"
              : "已停止"}
        </span>
      </span>
    </>
  );
}

function RailCard({
  instance,
  index,
  position,
  id,
  active,
  interactive,
  hidden,
  choose,
}: {
  instance: PickerInstance;
  index: number;
  position: MotionValue<number>;
  id: string;
  active: boolean;
  interactive: boolean;
  hidden: boolean;
  choose: (instance: PickerInstance) => void;
}) {
  const x = useTransform(
    position,
    (value) => curvePose(carouselSlot(index, value)).x,
  );
  const y = useTransform(
    position,
    (value) => curvePose(carouselSlot(index, value)).y,
  );
  const scale = useTransform(
    position,
    (value) => curvePose(carouselSlot(index, value)).scale,
  );
  const opacity = useTransform(
    position,
    (value) => curvePose(carouselSlot(index, value)).opacity,
  );
  const display = useTransform(position, (value) =>
    curvePose(carouselSlot(index, value)).opacity > 0.01
      ? ("block" as const)
      : ("none" as const),
  );
  const pointerEvents = useTransform(position, (value) =>
    Math.abs(carouselSlot(index, value)) > 0.5 &&
    curvePose(carouselSlot(index, value)).opacity > 0.02
      ? ("auto" as const)
      : ("none" as const),
  );
  return (
    <motion.button
      className={`instance-option ${active ? "is-candidate" : ""}`}
      id={id}
      type="button"
      role="option"
      aria-selected={active}
      aria-hidden={hidden || undefined}
      aria-label={`${instance.name}，${instance.kind}，${instance.version || "未知版本"}`}
      tabIndex={-1}
      disabled={!interactive || hidden}
      style={{
        x,
        y,
        scale,
        opacity,
        display,
        pointerEvents: interactive && !hidden ? pointerEvents : "none",
      }}
      onClick={(event) => {
        if (event.button === 0) choose(instance);
      }}
    >
      <CardContents instance={instance} />
    </motion.button>
  );
}

export function InstanceSwitcher({
  instances,
  selected,
  open,
  loading,
  disabled,
  onOpenChange,
  onSelect,
  onEmpty,
}: Props) {
  const listId = useId();
  const container = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const current = instances.find((instance) => instance.name === selected);
  const selectedIndex = Math.max(
    0,
    instances.findIndex((instance) => instance.name === selected),
  );
  const [anchor, setAnchor] = useState(selectedIndex);
  const anchorRef = useRef(anchor);
  const position = useMotionValue(selectedIndex);
  const reducedMotion = useReducedMotion() ?? false;
  const wasOpen = useRef(false);
  const previousSelected = useRef(selected);
  const pendingStep = useRef(0);
  const wheelState = useRef({ total: 0, direction: 0, lastStep: -Infinity });
  const confirmationTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [confirming, setConfirming] = useState(false);
  const confirmingRef = useRef<{
    instance: PickerInstance;
    index: number;
  } | null>(null);
  const onSelectRef = useRef(onSelect);
  onSelectRef.current = onSelect;
  const closingRef = useRef(false);
  const fadeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const [hiddenName, setHiddenName] = useState<string | null>(null);
  const [frozenRail, setFrozenRail] = useState<PickerInstance[] | null>(null);
  const rosterRef = useRef(instances);
  rosterRef.current = instances;
  const railInstances = frozenRail || instances;
  const candidateIndex = Math.max(
    0,
    Math.min(railInstances.length - 1, anchor),
  );
  const candidate = railInstances[candidateIndex];
  const inventoryKey = instances.map((instance) => instance.name).join("\0");

  useEffect(() => {
    const newlyOpened = open && !wasOpen.current;
    const selectedChanged = selected !== previousSelected.current;
    if (newlyOpened) {
      if (fadeTimer.current) clearTimeout(fadeTimer.current);
      closingRef.current = false;
      confirmingRef.current = null;
      setFrozenRail(null);
      setHiddenName(null);
      const next = clampAnchor(
        selectedIndex + pendingStep.current,
        instances.length - 1,
      );
      pendingStep.current = 0;
      anchorRef.current = next;
      setAnchor(next);
      position.jump(next);
    } else if (open && !confirmingRef.current && !closingRef.current) {
      const next = clampAnchor(
        selectedChanged ? selectedIndex : anchorRef.current,
        instances.length - 1,
      );
      if (next !== anchorRef.current) {
        anchorRef.current = next;
        setAnchor(next);
      }
    } else if (!open) {
      position.stop();
    }
    wasOpen.current = open;
    previousSelected.current = selected;
  }, [
    open,
    selected,
    inventoryKey,
    selectedIndex,
    instances.length,
    position,
    reducedMotion,
  ]);

  useEffect(() => {
    if (!open || confirmingRef.current || closingRef.current) return;
    if (reducedMotion) {
      position.jump(anchor);
      return;
    }
    const controls = animate(position, anchor, {
      type: "spring",
      ...INSTANCE_SPRING,
    });
    return () => controls.stop();
  }, [anchor, open, position, reducedMotion]);

  const close = useCallback(() => {
    // Keep the exact rendered pose and roster throughout fade-out. Reposition only
    // on the next opening, when the rail is still invisible.
    position.stop();
    setFrozenRail((previous) => previous || [...rosterRef.current]);
    closingRef.current = !reducedMotion;
    if (fadeTimer.current) clearTimeout(fadeTimer.current);
    if (!reducedMotion)
      fadeTimer.current = setTimeout(
        () => {
          closingRef.current = false;
        },
        FADE_SECONDS * 1000 + 20,
      );
    onOpenChange(false);
    pendingStep.current = 0;
    wheelState.current = { total: 0, direction: 0, lastStep: -Infinity };
  }, [onOpenChange, position, reducedMotion]);

  const finishConfirmation = useCallback(() => {
    const pending = confirmingRef.current;
    if (!pending) return;
    confirmingRef.current = null;
    if (confirmationTimer.current) clearTimeout(confirmationTimer.current);
    position.jump(pending.index);
    setConfirming(false);
    setHiddenName(pending.instance.name);
    onSelectRef.current(pending.instance.name);
    close();
    trigger.current?.focus({ preventScroll: true });
  }, [close, position]);

  useEffect(
    () => () => {
      position.stop();
      if (confirmationTimer.current) clearTimeout(confirmationTimer.current);
      if (fadeTimer.current) clearTimeout(fadeTimer.current);
    },
    [position],
  );

  const advance = useCallback(
    (step: number) => {
      const next = clampAnchor(anchorRef.current + step, instances.length - 1);
      anchorRef.current = next;
      setAnchor(next);
    },
    [instances.length],
  );

  useEffect(() => {
    const target = container.current;
    if (!target) return;
    const wheel = (event: WheelEvent) => {
      if (
        disabled ||
        confirmingRef.current ||
        closingRef.current ||
        instances.length < 2 ||
        event.deltaY === 0
      )
        return;
      event.preventDefault();
      event.stopPropagation();
      trigger.current?.focus({ preventScroll: true });
      const direction = Math.sign(event.deltaY);
      const now = performance.now();
      const state = wheelState.current;
      if (direction !== state.direction) {
        state.total = 0;
        state.direction = direction;
        state.lastStep = -Infinity;
      }
      const delta =
        event.deltaY *
        (event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? 110 : 1);
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
  }, [disabled, instances.length, open, advance, onOpenChange]);

  useEffect(() => {
    if (!open) return;
    const dismiss = (event: PointerEvent) => {
      if (
        !confirmingRef.current &&
        !container.current?.contains(event.target as Node)
      )
        close();
    };
    const cancelOnEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || confirmingRef.current) return;
      event.preventDefault();
      event.stopPropagation();
      close();
      trigger.current?.focus({ preventScroll: true });
    };
    document.addEventListener("pointerdown", dismiss);
    document.addEventListener("keydown", cancelOnEscape);
    return () => {
      document.removeEventListener("pointerdown", dismiss);
      document.removeEventListener("keydown", cancelOnEscape);
    };
  }, [open, close]);

  const choose = (instance: PickerInstance) => {
    if (disabled || confirmingRef.current || closingRef.current) return;
    const index = railInstances.findIndex(
      (entry) => entry.name === instance.name,
    );
    if (index < 0) return;
    position.stop();
    // Move the shared rail, retaining every card and its relative slot until arrival.
    setFrozenRail([...railInstances]);
    confirmingRef.current = { instance, index };
    setConfirming(true);
    anchorRef.current = index;
    setAnchor(index);
    if (reducedMotion || Math.abs(index - position.get()) < 0.02) {
      finishConfirmation();
      return;
    }
    animate(position, index, {
      duration: ALIGN_SECONDS,
      ease: [0.22, 0.75, 0.18, 1],
      onComplete: finishConfirmation,
    });
    // A background window may throttle frames; keep confirmation from getting stuck.
    confirmationTimer.current = setTimeout(
      finishConfirmation,
      ALIGN_SECONDS * 1000 + 120,
    );
  };

  const displayed = open ? candidate || current : current;

  return (
    <div
      ref={container}
      className="instance-switcher"
      onPointerEnter={(event) => {
        if (
          event.pointerType !== "touch" &&
          !disabled &&
          instances.length > 1 &&
          !confirmingRef.current &&
          !closingRef.current
        ) {
          onOpenChange(true);
          if (document.activeElement === document.body)
            trigger.current?.focus({ preventScroll: true });
        }
      }}
      onKeyDown={(event) => {
        if (disabled || confirmingRef.current || closingRef.current) return;
        if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          close();
          trigger.current?.focus();
          return;
        }
        if (open && ["Enter", " "].includes(event.key) && candidate) {
          event.preventDefault();
          choose(candidate);
          return;
        }
        if (
          !["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key) ||
          !instances.length
        )
          return;
        event.preventDefault();
        if (!open) {
          onOpenChange(true);
          return;
        }
        if (event.key === "Home" || event.key === "End") {
          const next =
            event.key === "Home" ? 0 : Math.max(0, instances.length - 1);
          anchorRef.current = next;
          setAnchor(next);
        } else advance(event.key === "ArrowDown" ? 1 : -1);
      }}
    >
      <motion.div
        className="instance-popover"
        initial={false}
        animate={{
          opacity: open ? 1 : 0,
          visibility: open ? "visible" : "hidden",
        }}
        transition={{
          opacity: { duration: reducedMotion ? 0 : FADE_SECONDS },
          visibility: { delay: open || reducedMotion ? 0 : FADE_SECONDS },
        }}
        aria-hidden={!open}
      >
        <div
          className="instance-rail"
          data-interactive={open ? "true" : "false"}
        >
          <div
            className="instance-hover-bridge"
            style={{ pointerEvents: open ? "auto" : "none" }}
          />
        </div>
        <div
          className="instance-options"
          id={listId}
          role="listbox"
          aria-label="实例列表"
          aria-activedescendant={
            candidate ? `${listId}-${candidateIndex}` : undefined
          }
          data-anchor={anchor}
        >
          {railInstances.map((instance, index) => (
            <RailCard
              key={instance.name}
              instance={instance}
              index={index}
              position={position}
              id={`${listId}-${index}`}
              active={index === candidateIndex}
              hidden={instance.name === hiddenName}
              interactive={
                open && !disabled && !confirming && !closingRef.current
              }
              choose={choose}
            />
          ))}
        </div>
      </motion.div>
      <button
        ref={trigger}
        className={`selected-instance ${open && railInstances.length ? "is-previewing" : ""}`}
        type="button"
        disabled={disabled || loading || confirming}
        aria-label="选择实例"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        title="滚轮逐格浏览，左键选择实例"
        onClick={() => {
          if (closingRef.current) return;
          if (!instances.length) onEmpty();
          else if (open && candidate) choose(candidate);
          else if (instances.length > 1) onOpenChange(true);
        }}
      >
        {displayed && <InstanceLogo kind={displayed.kind} />}
        <span className="instance-version">
          {loading
            ? "…"
            : displayed?.version ||
              (displayed?.status === "installing" ? "Installing" : "—")}
        </span>
        <span className="instance-name">
          {displayed
            ? `${displayed.name} · ${displayed.kind}`
            : "选择或添加实例"}
        </span>
        {instances.length > 1 && (
          <span className="instance-count" aria-hidden="true">
            {instances.length} ◂
          </span>
        )}
      </button>
    </div>
  );
}
