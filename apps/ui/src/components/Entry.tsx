import type { Annotation } from "@zuruf/protocol";
import { Match, Show, Switch, createSignal } from "solid-js";
import type { AppStore } from "../lib/store";
import { shortUrl } from "../lib/text";
import { Icon, type IconName, Shot } from "./basics";

const TYPE_ICON: Record<Annotation["type"], IconName> = {
  viewport: "viewport",
  region: "region",
  dom: "dom",
  text: "text",
};

const TYPE_NAME: Record<Annotation["type"], string> = {
  viewport: "Ansicht",
  region: "Ausschnitt",
  dom: "Element",
  text: "Kommentar",
};

/** What the entry points at, in the user's terms: the element's name, else the route. */
function target(annotation: Annotation): string {
  const dom = annotation.dom;
  if (dom) {
    const name = dom.name ?? dom.text;
    return name ? `${dom.role ?? dom.tag} „${name}“` : `<${dom.tag}>`;
  }
  return shortUrl(annotation.url);
}

/** One feedback entry of the open batch: thumbnail, editable words, where it points. */
export function Entry(props: { annotation: Annotation; store: AppStore; onOpenShot: (url: string) => void }) {
  const [focused, setFocused] = createSignal(false);
  const isText = () => props.annotation.type === "text";
  const field = () => (isText() ? "note" : "transcript");
  const value = () => (isText() ? props.annotation.note : props.annotation.transcript);
  const shot = () =>
    props.annotation.assets.find((asset) => asset.kind === "screenshot_marked") ??
    props.annotation.assets.find((asset) => asset.kind === "crop") ??
    props.annotation.assets[0];
  const unstable = () => {
    const best = props.annotation.dom?.locators[0];
    return best ? !best.stable : false;
  };

  const save = (element: HTMLTextAreaElement) => {
    setFocused(false);
    const next = element.value.trim();
    if (next !== value()) void props.store.commands.patchAnnotation(props.annotation.id, { [field()]: next });
  };

  return (
    <li class="entry" classList={{ "is-focused": focused() }}>
      <Show
        when={shot()}
        fallback={
          <span class="entry-thumb entry-thumb-text">
            <Icon name="text" />
          </span>
        }
      >
        {(asset) => (
          <button
            type="button"
            class="entry-thumb"
            aria-label={`${TYPE_NAME[props.annotation.type]} vergrößern`}
            onClick={() => props.onOpenShot(asset().url)}
          >
            <Shot url={asset().url} resolve={props.store.asset} alt="" />
            <span class="entry-type">
              <Icon name={TYPE_ICON[props.annotation.type]} size={12} />
            </span>
          </button>
        )}
      </Show>

      <div class="entry-body">
        <Switch>
          <Match when={props.annotation.transcriptState === "recording" && !value()}>
            <p class="entry-live">hört zu …</p>
          </Match>
          <Match when={props.annotation.transcriptState === "transcribing" && !value()}>
            <p class="entry-live">schreibt …</p>
          </Match>
          <Match when={true}>
            <textarea
              class="entry-text"
              rows={1}
              value={value()}
              placeholder={props.annotation.transcriptState === "failed" ? "Nicht verstanden – hier tippen" : "Anmerkung"}
              aria-label="Anmerkung"
              onFocus={() => setFocused(true)}
              onBlur={(event) => save(event.currentTarget)}
              onKeyDown={(event) => {
                if (event.key === "Escape" || (event.key === "Enter" && !event.shiftKey)) {
                  event.preventDefault();
                  event.currentTarget.blur();
                }
              }}
            />
          </Match>
        </Switch>
        <p class="entry-meta">
          <span class="entry-target">{target(props.annotation)}</span>
          <Show when={unstable()}>
            <span class="entry-flag" title="Kein stabiler Locator gefunden">
              unsicher
            </span>
          </Show>
        </p>
      </div>

      <button
        type="button"
        class="entry-remove"
        aria-label="Eintrag entfernen"
        onClick={() => props.store.commands.removeAnnotation(props.annotation.id)}
      >
        <Icon name="close" size={14} />
      </button>
    </li>
  );
}
