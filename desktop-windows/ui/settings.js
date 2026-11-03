const { invoke } = window.__TAURI__.core;

const $ = (id) => document.getElementById(id);
const FIELDS = ['url', 'token', 'light_label', 'switch_label', 'climate_label'];

function showError(message) {
  $('error').hidden = !message;
  $('error').textContent = message || '';
}

// Instant-apply: every edit is persisted right away (SPEC). Commands run in call order.
function save() {
  const form = {};
  for (const f of FIELDS) form[f] = $(f).value;
  invoke('save_settings', { form }).then(showError, (e) => showError(String(e)));
}

async function init() {
  const s = await invoke('get_settings');
  for (const f of FIELDS) $(f).value = s.config[f] ?? '';
  $('autostart').checked = s.autostart;
  showError(s.error);
  for (const f of FIELDS) $(f).addEventListener('input', save);
  $('url').focus();
}

$('reveal').addEventListener('click', () => {
  const hidden = $('token').type === 'password';
  $('token').type = hidden ? 'text' : 'password';
  $('reveal').textContent = hidden ? 'Hide' : 'Show';
});

$('autostart').addEventListener('change', async (ev) => {
  try {
    ev.target.checked = await invoke('set_autostart', { enabled: ev.target.checked });
    showError(null);
  } catch (e) {
    ev.target.checked = !ev.target.checked;
    showError(String(e));
  }
});

document.addEventListener('contextmenu', (ev) => {
  if (!ev.target.matches('input')) ev.preventDefault();
});

init().catch((e) => showError(String(e)));
