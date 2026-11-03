const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

// Entity id -> text being typed into a temperature field. Applied on Enter only (SPEC).
const edits = new Map();

const $ = (id) => document.getElementById(id);
const roomsEl = $('rooms');
const bannerEl = $('banner');
const scrollEl = $('scroll');
const footerEl = $('footer');

const CHEVRON_UP = '<svg viewBox="0 0 10 10"><path d="M2 6.5 5 3.5 8 6.5"/></svg>';
const CHEVRON_DOWN = '<svg viewBox="0 0 10 10"><path d="M2 3.5 5 6.5 8 3.5"/></svg>';

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}

/** Keyed child reconciliation: keeps DOM nodes (focus, caret, transitions) across updates. */
function sync(parent, items, keyOf, create, update) {
  const existing = new Map();
  for (const child of parent.children) existing.set(child.dataset.key, child);
  let prev = null;
  for (const item of items) {
    const key = keyOf(item);
    let node = existing.get(key);
    if (node) {
      existing.delete(key);
    } else {
      node = create(item);
      node.dataset.key = key;
    }
    update(node, item);
    const wanted = prev ? prev.nextSibling : parent.firstChild;
    if (node !== wanted) parent.insertBefore(node, wanted);
    prev = node;
  }
  for (const stale of existing.values()) stale.remove();
}

// ---- toggle (lights, switches) ----
function createToggle() {
  const row = el('div', 'item');
  const name = el('span', 'name');
  const label = el('label', 'switch');
  const input = el('input');
  input.type = 'checkbox';
  label.append(input, el('span', 'track'));
  row.append(name, label);
  input.addEventListener('change', () => invoke('toggle', { id: row.dataset.key }));
  row._parts = { name, input };
  return row;
}
function updateToggle(row, e) {
  row._parts.name.textContent = e.name;
  row._parts.name.title = e.name;
  row._parts.input.checked = e.on;
}

// ---- climate ----
function createClimate() {
  const row = el('div', 'item');
  const name = el('span', 'name');
  const controls = el('div', 'climate');
  const cur = el('span', 'cur');

  const box = el('div', 'field tempbox');
  const input = el('input');
  input.type = 'text';
  input.inputMode = 'decimal';
  input.autocomplete = 'off';
  input.spellcheck = false;
  box.append(input, el('span', 'unit', '°C'));

  const steppers = el('div', 'steppers');
  const up = el('button', 'step');
  const down = el('button', 'step');
  up.type = down.type = 'button';
  up.title = 'Warmer';
  down.title = 'Cooler';
  up.innerHTML = CHEVRON_UP;
  down.innerHTML = CHEVRON_DOWN;
  steppers.append(up, down);

  controls.append(cur, box, steppers);
  row.append(name, controls);

  input.addEventListener('input', () => edits.set(row.dataset.key, input.value));
  input.addEventListener('keydown', (ev) => {
    if (ev.key !== 'Enter') return;
    ev.preventDefault();
    const id = row.dataset.key;
    const text = edits.get(id);
    edits.delete(id);
    // Always answered with a fresh "view" event, which also restores the field if the text was invalid.
    if (text !== undefined) invoke('temp_submit', { id, text });
  });
  const step = (dir) => {
    const id = row.dataset.key;
    edits.delete(id);
    invoke('temp_step', { id, dir });
  };
  up.addEventListener('click', () => step(1));
  down.addEventListener('click', () => step(-1));

  row._parts = { name, cur, input };
  return row;
}
function updateClimate(row, e) {
  const { name, cur, input } = row._parts;
  name.textContent = e.name;
  name.title = e.name;
  cur.textContent = e.cur;
  cur.className = 'cur' + (e.action === 'heating' ? ' heating' : e.action === 'cooling' ? ' cooling' : '');
  const shown = edits.has(row.dataset.key) ? edits.get(row.dataset.key) : e.target;
  if (input.value !== shown) input.value = shown;
}

// ---- rooms ----
function createRoom() {
  const room = el('section', 'room');
  const cols = el('div', 'cols');
  for (const kind of ['lights', 'switches', 'climate']) {
    const cell = el('div', 'cell');
    cell.dataset.kind = kind;
    cols.append(cell);
  }
  room.append(el('h2', 'room-title'), el('div', 'rule'), cols);
  return room;
}
function updateRoom(node, room) {
  node.querySelector('.room-title').textContent = room.name;
  for (const cell of node.querySelectorAll('.cell')) {
    const kind = cell.dataset.kind;
    const climate = kind === 'climate';
    sync(cell, room[kind], (e) => e.id, climate ? createClimate : createToggle, climate ? updateClimate : updateToggle);
  }
}

function render(view) {
  bannerEl.hidden = !view.error;
  bannerEl.textContent = view.error || '';
  sync(roomsEl, view.rooms, (r) => r.name, createRoom, updateRoom);
  reportHeight();
}

// ---- window height: the native window follows the content, capped by the work area in Rust ----
let lastHeight = 0;
function reportHeight() {
  requestAnimationFrame(() => {
    const cs = getComputedStyle(scrollEl);
    const pad = parseFloat(cs.paddingTop) + parseFloat(cs.paddingBottom);
    const h = Math.ceil(bannerEl.offsetHeight + pad + roomsEl.offsetHeight + footerEl.offsetHeight);
    if (h !== lastHeight) {
      lastHeight = h;
      invoke('set_popup_height', { height: h });
    }
  });
}
const resizeObserver = new ResizeObserver(reportHeight);
[roomsEl, bannerEl, footerEl].forEach((n) => resizeObserver.observe(n));

// ---- wiring ----
let lastView = { rooms: [], error: null };
listen('view', (event) => {
  lastView = event.payload;
  render(lastView);
});
listen('popup-shown', () => {
  edits.clear();
  scrollEl.scrollTop = 0;
  render(lastView); // drop stale edit text immediately, then refresh from Home Assistant
  invoke('refresh');
});

$('open-settings').addEventListener('click', () => invoke('open_settings'));
document.addEventListener('keydown', (ev) => {
  if (ev.key === 'Escape') invoke('hide_popup');
});
document.addEventListener('contextmenu', (ev) => ev.preventDefault());
