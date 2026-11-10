import { type JSX, Show, createSignal, onCleanup } from "solid-js";

/**
 * Small anchored popover with outside-click and Escape dismissal.
 * Opens upwards or downwards; origin-aware scale so it grows out of its trigger.
 */
export function Menu(props: {
  trigger: (toggle: () => void, open: () => boolean) => JSX.Element;
  children: (close: () => void) => JSX.Element;
  placement?: "top" | "bottom";
  align?: "start" | "end";
}) {
  const [open, setOpen] = createSignal(false);
  let root!: HTMLSpanElement;

  const close = () => setOpen(false);
  const onPointer = (event: PointerEvent) => {
    if (!root.contains(event.target as Node)) close();
  };
  const onKey = (event: KeyboardEvent) => {
    if (event.key === "Escape") {
      event.stopPropagation();
      close();
    }
  };

  const toggle = () => {
    const next = !open();
    setOpen(next);
    if (next) {
      document.addEventListener("pointerdown", onPointer, true);
      document.addEventListener("keydown", onKey, true);
    } else {
      detach();
    }
  };
  const detach = () => {
    document.removeEventListener("pointerdown", onPointer, true);
    document.removeEventListener("keydown", onKey, true);
  };
  onCleanup(detach);

  return (
    <span class="menu" ref={root}>
      {props.trigger(toggle, open)}
      <Show when={open()}>
        <div
          class={`menu-pop menu-${props.placement ?? "bottom"} menu-${props.align ?? "start"}`}
          role="menu"
          onClick={(event) => {
            if ((event.target as HTMLElement).closest("[data-close]")) {
              close();
              detach();
            }
          }}
        >
          {props.children(() => {
            close();
            detach();
          })}
        </div>
      </Show>
    </span>
  );
}
