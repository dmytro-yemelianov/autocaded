// One pointer route for mouse, pen, touch taps, sketch drags and navigation.
export function installCanvasInput(canvas, { session, redraw, zoom, onError }) {
  const touches = new Map();
  let gesture = false;
  let geometry = null;

  const point = (event) => {
    const rect = canvas.getBoundingClientRect();
    return [(event.clientX - rect.left) * canvas.width / rect.width,
      (event.clientY - rect.top) * canvas.height / rect.height];
  };
  const run = (action) => {
    const cad = session();
    if (!cad) return;
    try { action(cad); } catch (error) { onError(error); }
    redraw();
  };
  const idle = (cad) => {
    const state = JSON.parse(cad.get_state_json());
    return state.command_idle && !state.input && !state.report_visible;
  };
  const pair = () => {
    if (touches.size < 2) return null;
    const [a, b] = [...touches.values()].slice(0, 2).map((touch) => touch.last);
    return { center: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2],
      distance: Math.hypot(a[0] - b[0], a[1] - b[1]) };
  };

  canvas.addEventListener('pointerdown', (event) => {
    if (!session() || event.button !== 0) return;
    canvas.setPointerCapture(event.pointerId);
    const start = point(event);
    if (event.pointerType !== 'touch') {
      run((cad) => cad.press(...start, canvas.width, canvas.height));
      return;
    }
    event.preventDefault();
    // A tap is committed on release, after a second finger can identify a
    // navigation gesture. Sketching starts only after a deliberate drag.
    touches.set(event.pointerId, { start, last: start,
      client: [event.clientX, event.clientY], moved: false, sketch: false });
    if (touches.size > 1) {
      gesture = true;
      for (const touch of touches.values()) {
        if (touch.sketch) {
          run((cad) => cad.release(...touch.last, canvas.width, canvas.height));
          touch.sketch = false;
        }
      }
      geometry = pair();
    }
  });

  canvas.addEventListener('pointermove', (event) => {
    const current = point(event);
    if (event.pointerType !== 'touch') {
      run((cad) => {
        cad.motion(...current, canvas.width, canvas.height);
        cad.cursor(...current);
      });
      return;
    }
    const touch = touches.get(event.pointerId);
    if (!touch) return;
    event.preventDefault();
    const previous = touch.last;
    touch.last = current;
    if (gesture) {
      const next = pair();
      if (next && geometry) {
        run((cad) => {
          if (!idle(cad)) return;
          cad.pan(next.center[0] - geometry.center[0], next.center[1] - geometry.center[1],
            canvas.width, canvas.height);
          if (geometry.distance > 0 && next.distance > 0 && next.distance !== geometry.distance) {
            zoom(next.distance / geometry.distance);
          }
        });
      }
      geometry = next;
      return;
    }
    const moved = Math.hypot(event.clientX - touch.client[0], event.clientY - touch.client[1]) >= 6;
    if (!touch.moved && !moved) return;
    const from = touch.moved ? previous : touch.start;
    touch.moved = true;
    run((cad) => {
      if (idle(cad)) {
        cad.pan(current[0] - from[0], current[1] - from[1], canvas.width, canvas.height);
      } else if (JSON.parse(cad.get_state_json()).sketch_active) {
        if (!touch.sketch) {
          cad.press(...touch.start, canvas.width, canvas.height);
          touch.sketch = true;
        }
        cad.motion(...current, canvas.width, canvas.height);
      }
      cad.cursor(...current);
    });
  });

  const finish = (event, cancelled) => {
    const current = point(event);
    if (event.pointerType !== 'touch') {
      if (event.button === 0 || cancelled) {
        run((cad) => cad.release(...current, canvas.width, canvas.height));
      }
      return;
    }
    const touch = touches.get(event.pointerId);
    if (!touch) return;
    event.preventDefault();
    run((cad) => {
      if (touch.sketch) {
        cad.release(...current, canvas.width, canvas.height);
      } else if (!cancelled && !gesture && !touch.moved) {
        cad.press(...current, canvas.width, canvas.height);
        cad.release(...current, canvas.width, canvas.height);
      }
    });
    touches.delete(event.pointerId);
    geometry = pair();
    // Keep suppressing the remaining finger after a pinch until all lift.
    if (touches.size === 0) gesture = false;
  };
  canvas.addEventListener('pointerup', (event) => finish(event, false));
  canvas.addEventListener('pointercancel', (event) => finish(event, true));
}

export function installModalKeyboard(document, modal, close) {
  document.addEventListener('keydown', (event) => {
    if (modal.hidden) return;
    // Capture before the canvas/soft keyboard handlers and window bubbling.
    event.stopPropagation();
    if (event.key === 'Escape') {
      event.preventDefault();
      close();
    }
  }, true);
}
