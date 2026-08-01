import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';
import vm from 'node:vm';
import { fileURLToPath } from 'node:url';

const root = path.dirname(fileURLToPath(import.meta.url));
const html = fs.readFileSync(path.join(root, 'index.html'), 'utf8');
const css = fs.readFileSync(path.join(root, 'style.css'), 'utf8');
const js = fs.readFileSync(path.join(root, 'main.js'), 'utf8');

assert.match(html, /class="slot is-sel" data-i="3" aria-current="true"/);
assert.match(html, /role="status" aria-live="polite" aria-atomic="true"/);
assert.match(html, /class="live-state">thinking</);
assert.equal((html.match(/<button class="chip/g) || []).length, 6);
assert.equal((html.match(/<button class="chip[^>]*type="button"/g) || []).length, 6);
assert.match(html, /class="nav-brand"[\s\S]*class="nav-word">omegaG</);
assert.doesNotMatch(css, /\.nav-word\s*\{[^}]*display:\s*none/s);
assert.doesNotMatch(css, /\.nav-links a:nth-child/);
for (const anchor of ['#lightbar', '#layer', '#specs']) assert.match(html, new RegExp(`href="${anchor}"`));
assert.doesNotMatch(js, /mouseenter|addEventListener\('focus'/);
assert.doesNotMatch(js, /classList\.toggle\('is-sel'/);
assert.match(html, /preserved Windows path[\s\S]*\$XDG_CONFIG_HOME\/ds4cc\/config\.toml[\s\S]*falling back to[\s\S]*~\/\.config\/ds4cc\/config\.toml/);
assert.match(html, /when a voice command is configured, L2 controls push-to-talk/);
assert.match(html, /Keyboard optional when configured/);
assert.match(html, /Share and Options are unmapped by default/);
assert.match(html, /Touchpad button<\/b>unmapped when touchpad mode is disabled/);
assert.match(html, /On DualSense, touchpad swipe moves the cursor; DualShock 4 uses left-stick mouse because coordinates are unsupported\. On both controllers, touchpad press clicks while touchpad handling is enabled\./);
assert.match(html, /<b>Left stick \/ DualSense touchpad swipe<\/b>mouse cursor/);
assert.match(html, /<b>Touchpad press<\/b>left-click on DualSense and DS4/);
assert.match(html, /optional Windows-only Codex runtime enabled/);
assert.match(html, /website illustration[\s\S]*not a live controller or HID connection/);
assert.match(html, /assets\/hero-journey\.png/);
assert.match(html, /class="hero-art"/);
// Badge branding only — allow "Claude Code" product mentions in body copy
assert.doesNotMatch(html, /Claude DS4\/5|["']CLAUDE["']|\bCLAUDE\b(?!\s*Code)/);
assert.match(html, /<meta property="og:image" content="https:\/\/veigapunk\.github\.io\/omegag-site\/assets\/hero-journey\.png">/);
assert.match(html, /<meta property="og:url" content="https:\/\/veigapunk\.github\.io\/omegag-site\/">/);
assert.match(html, /rel="canonical" href="https:\/\/veigapunk\.github\.io\/omegag-site\/"/);
assert.match(html, /masterpiece\.png"[^>]*width="1920" height="840"[^>]*loading="lazy"/);
assert.match(html, /controller-mark\.png"[^>]*width="128" height="128"/);
assert.match(css, /\.hero-art\s*\{[^}]*max-width:\s*100%[^}]*height:\s*auto/s);
assert.match(css, /\.device-art\s*\{[^}]*max-width:\s*100%[^}]*height:\s*auto/s);
assert.match(html, />Upstream DS4CC releases<\/a>/);
assert.match(html, /Package and binary name remain <code>ds4cc<\/code>/);
assert.match(html, /Same package and binary \(<code>ds4cc<\/code>\)/);
assert.doesNotMatch(html, />Releases<\/a>/);
assert.match(html, /Windows installer \(upstream\)/);
assert.match(html, /Linux build/);
assert.match(html, /href="https:\/\/github\.com\/VeigaPunk\/DS4CC\/releases\/latest"/);
assert.match(html, /href="https:\/\/github\.com\/vgpnk-holdings-llc\/omegaG#quick-start"/);

for (const [, ref] of html.matchAll(/(?:src|href)="([^"#]+)"/g)) {
  if (/^[a-z][a-z\d+.-]*:/i.test(ref)) continue;
  const localPath = ref.split(/[?#]/, 1)[0];
  assert.ok(fs.existsSync(path.join(root, localPath)), `missing local reference: ${ref}`);
}
assert.ok(!fs.existsSync(path.join(root, 'assets/logo-badge.png')), 'unused logo-badge.png remains');
assert.ok(fs.existsSync(path.join(root, 'assets/hero-journey.png')), 'omegaG hero badge missing');

class ClassList {
  constructor(names = []) { this.names = new Set(names); }
  add(name) { this.names.add(name); }
  remove(name) { this.names.delete(name); }
  contains(name) { return this.names.has(name); }
}
class Element {
  constructor(attrs = {}, classes = []) {
    this.attrs = { ...attrs };
    this.classList = new ClassList(classes);
    this.listeners = {};
    this.style = { setProperty: (name, value) => { this.style[name] = value; } };
    this.textContent = '';
  }
  getAttribute(name) { return this.attrs[name] ?? null; }
  setAttribute(name, value) { this.attrs[name] = value; }
  addEventListener(name, listener) { (this.listeners[name] ||= []).push(listener); }
  activate(name) { for (const listener of this.listeners[name] || []) listener(); }
}

const states = [
  ['idle', '#f4f4f2'], ['thinking', '#3b82f6'], ['complete-unread', '#34d399'],
  ['requires-input', '#f5a623'], ['error', '#ef4444'], ['unassigned', '']
];
const chips = states.map(([state, color], i) => new Element(
  { 'data-state': state, 'data-color': color, 'aria-pressed': i === 1 ? 'true' : 'false' },
  i === 1 ? ['chip', 'is-on'] : ['chip']
));
const slots = Array.from({ length: 6 }, (_, i) => new Element(
  { 'data-i': String(i + 1) }, i === 2 ? ['slot', 'is-sel'] : ['slot']
));
const bar = new Element({}, ['lightbar']);
const etch = new Element();
const live = new Element();
const stage = new Element();
stage.querySelector = selector => ({ '.lightbar': bar, '.etch-state': etch, '.live-state': live }[selector]);
stage.querySelectorAll = selector => selector === '.chip' ? chips : selector === '.slot' ? slots : [];

vm.runInNewContext(js, { document: { querySelector: selector => selector === '.lb-stage' ? stage : null } });
assert.deepEqual(Object.keys(chips[0].listeners), ['click'], 'status must change on activation only');
assert.equal(slots.findIndex(slot => slot.classList.contains('is-sel')) + 1, 3);
assert.equal(etch.textContent, 'thinking');
assert.equal(live.textContent, 'thinking');
chips[4].activate('click');
assert.equal(slots.findIndex(slot => slot.classList.contains('is-sel')) + 1, 3, 'status activation changed selected slot');
assert.equal(etch.textContent, 'error');
assert.equal(live.textContent, 'error');
assert.equal(chips.filter(chip => chip.attrs['aria-pressed'] === 'true').length, 1);
assert.equal(chips[4].attrs['aria-pressed'], 'true');

console.log('website checks: pass');
