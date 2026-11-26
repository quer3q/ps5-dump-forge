// A browser-style device tab strip (WAI-ARIA tabs; offline tabs greyed and skipped); the chosen tab joins a `.dev-window` card.

import { useEffect, type KeyboardEvent } from "react";

import "./devices.css";
import { panelId, tabId } from "./common";
import { useMessages } from "./i18n";
import { Icon, type IconName } from "./icons";

export interface DeviceTab {
  id: string;
  name: string;
  icon: IconName;
  online: boolean;
  tag?: string;
}

/** What a tab's panel spreads on itself, so the tab names it. */
export function tabPanelProps(prefix: string, id: string) {
  return { role: "tabpanel", id: panelId(prefix, id), "aria-labelledby": tabId(prefix, id) } as const;
}

export function DeviceTabs(props: {
  label: string;
  /** Element ids: `tabId(prefix, id)`, its panel `panelId(prefix, id)` (`tabPanelProps`). */
  prefix: string;
  tabs: DeviceTab[];
  value: string;
  onChange: (id: string) => void;
  onAdd?: () => void;
}) {
  const t = useMessages().deviceTabs;
  const { prefix, value } = props;
  const live = props.tabs.filter((d) => d.online).map((d) => d.id);
  // The tab stop: the chosen tab, else the first online one.
  const stop = live.includes(value) ? value : live[0];
  // By hand, not scrollIntoView: older WebKit takes its options as "align to top" and scrolls the page.
  useEffect(() => {
    const tab = document.getElementById(tabId(prefix, value));
    const bar = tab?.parentElement;
    if (!tab || !bar || bar.scrollWidth <= bar.clientWidth) return;
    const a = tab.getBoundingClientRect();
    const b = bar.getBoundingClientRect();
    if (a.left < b.left) bar.scrollLeft -= b.left - a.left;
    else if (a.right > b.right) bar.scrollLeft += a.right - b.right;
  }, [prefix, value]);
  const onKey = (e: KeyboardEvent) => {
    if (live.length === 0) return;
    const i = live.indexOf(stop);
    // Right-to-left, the next tab is to the left.
    const rtl = document.documentElement.dir === "rtl";
    const next =
      e.key === (rtl ? "ArrowLeft" : "ArrowRight") ? live[(i + 1) % live.length]
      : e.key === (rtl ? "ArrowRight" : "ArrowLeft") ? live[(i - 1 + live.length) % live.length]
      : e.key === "Home" ? live[0]
      : e.key === "End" ? live[live.length - 1]
      : undefined;
    if (next === undefined) return;
    e.preventDefault();
    props.onChange(next);
    document.getElementById(tabId(prefix, next))?.focus();
  };
  return (
    <div className="dev-tabs-row">
      <div className="dev-tabs" role="tablist" aria-label={props.label} onKeyDown={onKey}>
        {props.tabs.map((d) => {
          const on = d.id === value;
          return (
            <button
              key={d.id}
              type="button"
              role="tab"
              id={tabId(prefix, d.id)}
              aria-selected={on}
              aria-controls={panelId(prefix, d.id)}
              aria-disabled={!d.online || undefined}
              tabIndex={d.id === stop ? 0 : -1}
              className={d.online ? "dev-tab" : "dev-tab offline"}
              title={d.online ? d.name : t.offlineNamed(d.name)}
              onClick={() => d.online && !on && props.onChange(d.id)}
            >
              <Icon name={d.icon} />
              <span className="dev-tab-name">{d.name}</span>
              {d.tag && <span className="tag">{d.tag}</span>}
              <span className="dev-tab-dot" aria-hidden="true" />
              {!d.online && <span className="visually-hidden">{t.offline}</span>}
            </button>
          );
        })}
      </div>
      {props.onAdd && (
        <button type="button" className="dev-tab-add" title={t.add} aria-label={t.add} onClick={props.onAdd}>
          <Icon name="plus" />
        </button>
      )}
    </div>
  );
}
