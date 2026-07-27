/* omegaG — lightbar demo: selected-slot state projection.
   States from the runtime spec: idle / thinking / complete-unread /
   requires-input / error / unassigned. Selected projection pulses at 500 ms. */
(function () {
  var stage  = document.querySelector('.lb-stage');
  if (!stage) return;

  var bar    = stage.querySelector('.lightbar');
  var chips  = Array.prototype.slice.call(stage.querySelectorAll('.chip'));
  var slots  = Array.prototype.slice.call(stage.querySelectorAll('.slot'));
  var etch   = stage.querySelector('.etch-state');

  function hexToGlow(hex) {
    if (!hex) return 'transparent';
    var r = parseInt(hex.slice(1, 3), 16);
    var g = parseInt(hex.slice(3, 5), 16);
    var b = parseInt(hex.slice(5, 7), 16);
    return 'rgba(' + r + ',' + g + ',' + b + ',0.45)';
  }

  function select(chip, idx) {
    var color = chip.getAttribute('data-color');
    var state = chip.getAttribute('data-state');

    chips.forEach(function (c) {
      c.classList.remove('is-on');
      c.setAttribute('aria-pressed', 'false');
    });
    chip.classList.add('is-on');
    chip.setAttribute('aria-pressed', 'true');

    slots.forEach(function (s, i) { s.classList.toggle('is-sel', i === idx); });

    if (color) {
      stage.style.setProperty('--lb', color);
      stage.style.setProperty('--lb-glow', hexToGlow(color));
      bar.classList.remove('off');
      bar.classList.add('pulsing');      // selected slot pulses at 500 ms
    } else {
      // unassigned — lightbar off
      bar.classList.remove('pulsing');
      bar.classList.add('off');
    }

    if (etch) etch.textContent = state;
  }

  chips.forEach(function (chip, idx) {
    chip.style.setProperty('--c', chip.getAttribute('data-color') || '#3a3a3f');
    chip.addEventListener('mouseenter', function () { select(chip, idx); });
    chip.addEventListener('focus',      function () { select(chip, idx); });
    chip.addEventListener('click',      function () { select(chip, idx); });
  });

  // initial state: thinking on slot 3
  select(chips[1], 1);
})();
