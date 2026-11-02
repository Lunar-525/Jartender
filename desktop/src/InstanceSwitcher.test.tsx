// @vitest-environment jsdom
import React, { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  act,
} from "@testing-library/react";
import { animate, useReducedMotion } from "motion/react";
import { InstanceSwitcher, type PickerInstance } from "./InstanceSwitcher";

const motionProbe = vi.hoisted(() => ({
  position: null as import("motion/react").MotionValue<number> | null,
}));

vi.mock("motion/react", async (importOriginal) => {
  const actual = await importOriginal<typeof import("motion/react")>();
  return {
    ...actual,
    useReducedMotion: vi.fn(() => true),
    animate: vi.fn(actual.animate),
    useMotionValue: (value: number) => {
      const position = actual.useMotionValue(value);
      motionProbe.position = position;
      return position;
    },
  };
});
afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.restoreAllMocks();
  vi.mocked(animate).mockClear();
  vi.mocked(useReducedMotion).mockReturnValue(true);
});

const instances: PickerInstance[] = Array.from({ length: 20 }, (_, index) => ({
  name: `server-${index}`,
  version: `1.21.${index}`,
  kind: "Fabric",
  status: index === 3 ? "running" : "stopped",
}));
function Harness({
  entries = instances,
  initiallyOpen = false,
  onSelect = vi.fn(),
}: {
  entries?: PickerInstance[];
  initiallyOpen?: boolean;
  onSelect?: (name: string) => void;
}) {
  const [open, setOpen] = useState(initiallyOpen);
  const [selected, setSelected] = useState("server-6");
  return (
    <InstanceSwitcher
      instances={entries}
      selected={selected}
      open={open}
      loading={false}
      disabled={false}
      onOpenChange={setOpen}
      onSelect={(name) => {
        onSelect(name);
        setSelected(name);
      }}
      onEmpty={() => {}}
    />
  );
}
const trigger = () => screen.getByRole("button", { name: "选择实例" });
const anchor = () =>
  Number(
    screen.getByRole("listbox", { hidden: true }).getAttribute("data-anchor"),
  );
const nearOption = () =>
  screen.getByRole("option", { name: "server-5，Fabric，1.21.5" });

const alignment = () =>
  [...vi.mocked(animate).mock.calls]
    .reverse()
    .find(
      (call) =>
        call[2] && typeof call[2] === "object" && "onComplete" in call[2],
    )!;
const completeAlignment = () => {
  const options = alignment()[2] as { onComplete: () => void };
  act(() => options.onComplete());
};

describe("curved instance carousel", () => {
  it("previews the next instance in the frame without committing it", async () => {
    const selected = vi.fn();
    render(<Harness onSelect={selected} />);
    const wheel = new WheelEvent("wheel", {
      deltaY: 10000,
      bubbles: true,
      cancelable: true,
    });
    fireEvent(trigger(), wheel);
    expect(wheel.defaultPrevented).toBe(true);
    expect(anchor()).toBe(7);
    expect(selected).not.toHaveBeenCalled();
    expect(trigger().textContent).toContain("server-7");
    expect(
      (
        await screen.findByRole("option", { name: "server-7，Fabric，1.21.7" })
      ).getAttribute("aria-selected"),
    ).toBe("true");
  });
  it("accumulates small trackpad input and suppresses the immediate momentum tail", () => {
    render(<Harness initiallyOpen />);
    let now = 1000;
    vi.spyOn(performance, "now").mockImplementation(() => now);
    fireEvent.wheel(trigger(), { deltaY: 18 });
    expect(anchor()).toBe(6);
    fireEvent.wheel(trigger(), { deltaY: 24 });
    expect(anchor()).toBe(7);
    now += 20;
    fireEvent.wheel(trigger(), { deltaY: 80 });
    expect(anchor()).toBe(7);
    now += 120;
    fireEvent.wheel(trigger(), { deltaY: 80 });
    expect(anchor()).toBe(8);
    fireEvent.wheel(trigger(), { deltaY: -80 });
    expect(anchor()).toBe(7);
  });
  it("retains the snapped slot across inventory polling and clamps both ends", () => {
    const view = render(<Harness initiallyOpen />);
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    expect(anchor()).toBe(7);
    view.rerender(
      <Harness
        initiallyOpen
        entries={instances.map((entry) => ({ ...entry }))}
      />,
    );
    expect(anchor()).toBe(7);
    fireEvent.keyDown(trigger(), { key: "End" });
    expect(anchor()).toBe(19);
    fireEvent.wheel(trigger(), { deltaY: 10000 });
    expect(anchor()).toBe(19);
    fireEvent.keyDown(trigger(), { key: "Home" });
    expect(anchor()).toBe(0);
    fireEvent.wheel(trigger(), { deltaY: -10000 });
    expect(anchor()).toBe(0);
  });
  it("inserts a left-clicked card into the current-instance slot", () => {
    const selected = vi.fn();
    render(<Harness initiallyOpen onSelect={selected} />);
    fireEvent.click(nearOption(), { button: 0 });
    expect(selected).toHaveBeenCalledExactlyOnceWith("server-5");
    expect(trigger().textContent).toContain("server-5");
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(trigger());
  });
  it("slides the shared rail to a clicked instance before committing and fading", () => {
    vi.useFakeTimers();
    vi.mocked(useReducedMotion).mockReturnValue(false);
    const selected = vi.fn();
    const view = render(<Harness initiallyOpen onSelect={selected} />);
    const chosen = nearOption();
    fireEvent.click(chosen, { button: 0 });
    expect(anchor()).toBe(5);
    expect(trigger().getAttribute("aria-expanded")).toBe("true");
    expect(selected).not.toHaveBeenCalled();
    expect(trigger().textContent).toContain("server-5");
    expect(alignment()[0]).toBe(motionProbe.position);
    expect(alignment()[1]).toBe(5);
    expect(motionProbe.position!.get()).toBe(6);
    expect(chosen.hasAttribute("aria-hidden")).toBe(false);
    expect(view.container.querySelector(".instance-insertion")).toBeNull();
    fireEvent.click(
      view.container.querySelector(
        '.instance-option[aria-label="server-4，Fabric，1.21.4"]',
      )!,
    );
    fireEvent.keyDown(trigger(), { key: "Enter" });
    expect(
      vi
        .mocked(animate)
        .mock.calls.filter((call) => call[2] && "onComplete" in call[2]),
    ).toHaveLength(1);
    completeAlignment();
    expect(motionProbe.position!.get()).toBe(5);
    expect(selected).toHaveBeenCalledExactlyOnceWith("server-5");
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
    expect(chosen.getAttribute("aria-hidden")).toBe("true");
    act(() => vi.advanceTimersByTime(600));
    expect(motionProbe.position!.get()).toBe(5);
    expect(selected).toHaveBeenCalledTimes(1);
  });
  it("retains the roster during alignment and freezes its final pose throughout fade-out", () => {
    vi.useFakeTimers();
    vi.mocked(useReducedMotion).mockReturnValue(false);
    const selected = vi.fn();
    const view = render(<Harness initiallyOpen onSelect={selected} />);
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    // Enter while the wheel spring is still between detents.
    act(() => motionProbe.position!.jump(6.4));
    const chosen = view.container.querySelector(
      '.instance-option[aria-label="server-7，Fabric，1.21.7"]',
    );
    const railNames = () =>
      Array.from(view.container.querySelectorAll(".instance-option"), (card) =>
        card.getAttribute("aria-label"),
      );
    const originalOrder = railNames();
    fireEvent.keyDown(trigger(), { key: "Enter" });
    expect(selected).not.toHaveBeenCalled();
    expect(motionProbe.position!.get()).toBe(6.4);
    expect(trigger().getAttribute("aria-expanded")).toBe("true");
    act(() => motionProbe.position!.jump(6.8));
    view.rerender(
      <Harness
        initiallyOpen
        entries={[...instances].reverse()}
        onSelect={selected}
      />,
    );
    expect(railNames()).toEqual(originalOrder);
    expect(motionProbe.position!.get()).toBe(6.8);
    expect(trigger().textContent).toContain("server-7");
    completeAlignment();
    expect(selected).toHaveBeenCalledExactlyOnceWith("server-7");
    expect(motionProbe.position!.get()).toBe(7);
    expect(
      view.container.querySelector(
        '.instance-option[aria-label="server-7，Fabric，1.21.7"]',
      ),
    ).toBe(chosen);
    expect(chosen!.getAttribute("aria-hidden")).toBe("true");
    act(() => vi.advanceTimersByTime(100));
    fireEvent.pointerEnter(
      view.container.querySelector(".instance-switcher")!,
      { pointerType: "mouse" },
    );
    fireEvent.wheel(trigger(), { deltaY: 120 });
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
    expect(motionProbe.position!.get()).toBe(7);
    act(() => vi.advanceTimersByTime(500));
    expect(motionProbe.position!.get()).toBe(7);
    expect(railNames()).toEqual(originalOrder);
    fireEvent.click(trigger());
    expect(anchor()).toBe(12);
    expect(motionProbe.position!.get()).toBe(12);
    expect(railNames()).toEqual([...originalOrder].reverse());
    expect(chosen!.hasAttribute("aria-hidden")).toBe(false);
  });
  it("fades a settled rail immediately without moving it again", () => {
    vi.useFakeTimers();
    vi.mocked(useReducedMotion).mockReturnValue(false);
    render(<Harness initiallyOpen />);
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    act(() => motionProbe.position!.jump(7));
    fireEvent.click(trigger());
    expect(motionProbe.position!.get()).toBe(7);
    act(() => vi.advanceTimersByTime(500));
    expect(motionProbe.position!.get()).toBe(7);
    expect(trigger().textContent).toContain("server-7");
  });
  it("supports keyboard confirmation and Escape without accidental selection", () => {
    const selected = vi.fn();
    render(<Harness onSelect={selected} />);
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    expect(anchor()).toBe(6);
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    expect(trigger().textContent).toContain("server-7");
    fireEvent.keyDown(trigger(), { key: "Escape" });
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
    expect(selected).not.toHaveBeenCalled();
    expect(trigger().textContent).toContain("server-6");
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    expect(anchor()).toBe(6);
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    fireEvent.keyDown(trigger(), { key: "Enter" });
    expect(selected).toHaveBeenCalledExactlyOnceWith("server-7");
  });
  it("ignores non-left clicks and dismisses outside the rail", () => {
    const selected = vi.fn();
    render(<Harness initiallyOpen onSelect={selected} />);
    fireEvent.click(nearOption(), { button: 1 });
    expect(selected).not.toHaveBeenCalled();
    fireEvent.pointerDown(document.body);
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
  });
  it("confirms the frame preview itself with a left click", () => {
    const selected = vi.fn();
    render(<Harness initiallyOpen onSelect={selected} />);
    fireEvent.wheel(trigger(), { deltaY: 120 });
    expect(trigger().textContent).toContain("server-7");
    expect(selected).not.toHaveBeenCalled();
    fireEvent.click(trigger(), { button: 0 });
    expect(selected).toHaveBeenCalledExactlyOnceWith("server-7");
    expect(trigger().textContent).toContain("server-7");
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
  });
  it("keeps an unconfirmed preview open when the pointer leaves, and cancels with Escape", () => {
    const selected = vi.fn();
    const view = render(<Harness initiallyOpen onSelect={selected} />);
    fireEvent.wheel(trigger(), { deltaY: 120 });
    fireEvent.pointerLeave(view.container.querySelector(".instance-switcher")!);
    expect(trigger().getAttribute("aria-expanded")).toBe("true");
    expect(trigger().textContent).toContain("server-7");
    fireEvent.keyDown(document.body, { key: "Escape" });
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
    expect(trigger().textContent).toContain("server-6");
    expect(selected).not.toHaveBeenCalled();
  });
  it("does not capture wheel events when there are no alternative instances", () => {
    render(<Harness entries={[instances[6]]} />);
    const wheel = new WheelEvent("wheel", {
      deltaY: 120,
      bubbles: true,
      cancelable: true,
    });
    fireEvent(trigger(), wheel);
    expect(wheel.defaultPrevented).toBe(false);
  });
});
