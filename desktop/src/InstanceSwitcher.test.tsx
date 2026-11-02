// @vitest-environment jsdom
import React, { useState } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, act } from "@testing-library/react";
import { useReducedMotion } from "motion/react";
import { InstanceSwitcher, type PickerInstance } from "./InstanceSwitcher";

vi.mock("motion/react", async importOriginal => {
  const actual = await importOriginal<typeof import("motion/react")>();
  return { ...actual, useReducedMotion: vi.fn(() => true) };
});
afterEach(() => { cleanup(); vi.useRealTimers(); vi.restoreAllMocks(); vi.mocked(useReducedMotion).mockReturnValue(true); });

const instances: PickerInstance[] = Array.from({ length: 20 }, (_, index) => ({
  name: `server-${index}`, version: `1.21.${index}`, kind: "Fabric", status: index === 3 ? "running" : "stopped",
}));
function Harness({ entries = instances, initiallyOpen = false, onSelect = vi.fn() }: {
  entries?: PickerInstance[]; initiallyOpen?: boolean; onSelect?: (name: string) => void;
}) {
  const [open, setOpen] = useState(initiallyOpen);
  const [selected, setSelected] = useState("server-6");
  return <InstanceSwitcher instances={entries} selected={selected} open={open} loading={false} disabled={false}
    onOpenChange={setOpen} onSelect={name => { onSelect(name); setSelected(name); }} onEmpty={() => {}} />;
}
const trigger = () => screen.getByRole("button", { name: "选择实例" });
const anchor = () => Number(screen.getByRole("listbox", { name: "实例列表" }).getAttribute("data-anchor"));
const nearOption = () => screen.getByRole("option", { name: "server-5，Fabric，1.21.5" });

describe("curved instance carousel", () => {
  it("opens on a wheel gesture and advances one whole slot without selecting", () => {
    const selected = vi.fn(); render(<Harness onSelect={selected} />);
    const wheel = new WheelEvent("wheel", { deltaY: 10000, bubbles: true, cancelable: true });
    fireEvent(trigger(), wheel);
    expect(wheel.defaultPrevented).toBe(true);
    expect(anchor()).toBe(7);
    expect(selected).not.toHaveBeenCalled();
    expect(trigger().textContent).toContain("server-6");
  });
  it("accumulates small trackpad input and suppresses the immediate momentum tail", () => {
    render(<Harness initiallyOpen />);
    let now = 1000; vi.spyOn(performance, "now").mockImplementation(() => now);
    fireEvent.wheel(trigger(), { deltaY: 18 }); expect(anchor()).toBe(6);
    fireEvent.wheel(trigger(), { deltaY: 24 }); expect(anchor()).toBe(7);
    now += 20; fireEvent.wheel(trigger(), { deltaY: 80 }); expect(anchor()).toBe(7);
    now += 120; fireEvent.wheel(trigger(), { deltaY: 80 }); expect(anchor()).toBe(8);
    fireEvent.wheel(trigger(), { deltaY: -80 }); expect(anchor()).toBe(7);
  });
  it("retains the snapped slot across inventory polling and clamps both ends", () => {
    const view = render(<Harness initiallyOpen />);
    fireEvent.keyDown(trigger(), { key: "ArrowDown" }); expect(anchor()).toBe(7);
    view.rerender(<Harness initiallyOpen entries={instances.map(entry => ({ ...entry }))} />);
    expect(anchor()).toBe(7);
    fireEvent.keyDown(trigger(), { key: "End" }); expect(anchor()).toBe(19);
    fireEvent.wheel(trigger(), { deltaY: 10000 }); expect(anchor()).toBe(19);
    fireEvent.keyDown(trigger(), { key: "Home" }); expect(anchor()).toBe(0);
    fireEvent.wheel(trigger(), { deltaY: -10000 }); expect(anchor()).toBe(0);
  });
  it("inserts a left-clicked card into the current-instance slot", () => {
    const selected = vi.fn(); render(<Harness initiallyOpen onSelect={selected} />);
    fireEvent.click(nearOption(), { button: 0 });
    expect(selected).toHaveBeenCalledExactlyOnceWith("server-5");
    expect(trigger().textContent).toContain("server-5");
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
    expect(document.activeElement).toBe(trigger());
  });
  it("keeps a flying card until insertion finishes and prevents duplicate selection", () => {
    vi.useFakeTimers(); vi.mocked(useReducedMotion).mockReturnValue(false);
    const selected = vi.fn(); const view = render(<Harness initiallyOpen onSelect={selected} />);
    fireEvent.click(nearOption(), { button: 0 });
    expect(view.container.querySelector(".instance-insertion")?.textContent).toContain("server-5");
    expect(trigger().textContent).toContain("server-6");
    fireEvent.click(screen.getByRole("option", { name: "server-4，Fabric，1.21.4" }), { button: 0 });
    expect(selected).toHaveBeenCalledTimes(1);
    act(() => { vi.advanceTimersByTime(500); });
    expect(view.container.querySelector(".instance-insertion")).toBeNull();
    expect(trigger().textContent).toContain("server-5");
    expect(trigger().getAttribute("aria-expanded")).toBe("false");
  });
  it("supports keyboard confirmation and Escape without accidental selection", () => {
    const selected = vi.fn(); render(<Harness onSelect={selected} />);
    fireEvent.keyDown(trigger(), { key: "ArrowDown" });
    expect(anchor()).toBe(6);
    fireEvent.keyDown(trigger(), { key: "Escape" });
    expect(trigger().getAttribute("aria-expanded")).toBe("false"); expect(selected).not.toHaveBeenCalled();
    fireEvent.keyDown(trigger(), { key: "ArrowDown" }); fireEvent.keyDown(trigger(), { key: "Enter" });
    expect(selected).toHaveBeenCalledExactlyOnceWith("server-5");
  });
  it("ignores non-left clicks and dismisses outside the rail", () => {
    const selected = vi.fn(); render(<Harness initiallyOpen onSelect={selected} />);
    fireEvent.click(nearOption(), { button: 1 }); expect(selected).not.toHaveBeenCalled();
    fireEvent.pointerDown(document.body); expect(trigger().getAttribute("aria-expanded")).toBe("false");
  });
  it("does not capture wheel events when there are no alternative instances", () => {
    render(<Harness entries={[instances[6]]} />);
    const wheel = new WheelEvent("wheel", { deltaY: 120, bubbles: true, cancelable: true });
    fireEvent(trigger(), wheel); expect(wheel.defaultPrevented).toBe(false);
  });
});
