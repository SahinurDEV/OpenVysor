(() => {
  const root = document.documentElement;
  const calm = matchMedia("(prefers-reduced-motion: reduce)").matches;
  const $ = (selector, scope = document) => scope.querySelector(selector);
  const $$ = (selector, scope = document) => [...scope.querySelectorAll(selector)];
  const store = {
    get(key) { try { return localStorage.getItem(key); } catch { return null; } },
    set(key, value) { try { localStorage.setItem(key, value); } catch {} },
  };

  // Theme
  const saved = store.get("theme");
  if (saved) root.dataset.theme = saved;
  $$("[data-theme-toggle]").forEach((button) => button.addEventListener("click", () => {
    const dark = root.dataset.theme ? root.dataset.theme === "dark" : matchMedia("(prefers-color-scheme: dark)").matches;
    root.dataset.theme = dark ? "light" : "dark";
    store.set("theme", root.dataset.theme);
  }));

  // Top bar and reading progress
  const bar = $(".bar");
  const progress = $(".progress");
  const onScroll = () => {
    bar?.classList.toggle("scrolled", scrollY > 8);
    if (progress) {
      const room = document.body.scrollHeight - innerHeight;
      progress.style.setProperty("--p", room > 0 ? Math.min(scrollY / room, 1) : 0);
    }
    const shot = $(".shot");
    if (shot && !calm) {
      const box = shot.getBoundingClientRect();
      const left = Math.min(Math.max((box.top - innerHeight * 0.2) / (innerHeight * 0.7), 0), 1);
      shot.style.setProperty("--tilt", `${left * 14}deg`);
      shot.style.setProperty("--zoom", `${1 - left * 0.05}`);
    }
  };
  addEventListener("scroll", onScroll, { passive: true });
  onScroll();

  // Mobile menus
  $$("[data-toggle]").forEach((button) => button.addEventListener("click", () => {
    const open = $(button.dataset.toggle).classList.toggle("open");
    button.setAttribute("aria-expanded", String(open));
  }));
  $$(".links a, .side a").forEach((link) => link.addEventListener("click", () => {
    link.parentElement.closest(".links, .side")?.classList.remove("open");
  }));

  // Headline
  $$("[data-words]").forEach((title) => {
    let index = 0;
    const split = (node) => [...node.childNodes].forEach((child) => {
      if (child.nodeType !== Node.TEXT_NODE) return split(child);
      const part = document.createDocumentFragment();
      child.textContent.split(/(\s+)/).forEach((text) => {
        if (!text.trim()) return part.append(text);
        const word = document.createElement("span");
        word.className = "word";
        word.style.setProperty("--i", index++);
        word.textContent = text;
        part.append(word);
      });
      child.replaceWith(part);
    });
    split(title);
  });

  // Scroll reveals and counters
  const countUp = (element) => {
    const end = Number(element.dataset.count);
    const places = (element.dataset.count.split(".")[1] || "").length;
    if (calm) return;
    const started = performance.now();
    const tick = (now) => {
      const done = Math.min((now - started) / 1100, 1);
      element.firstChild.textContent = (end * (1 - (1 - done) ** 3)).toFixed(places);
      if (done < 1) requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  };
  const seen = new IntersectionObserver((entries) => entries.forEach((entry) => {
    if (!entry.isIntersecting) return;
    entry.target.classList.add("in");
    $$("[data-count]", entry.target).forEach(countUp);
    seen.unobserve(entry.target);
  }), { rootMargin: "0px 0px -8% 0px" });
  $$(".reveal").forEach((element) => seen.observe(element));

  // Spotlight on cards
  $$(".cell").forEach((cell) => cell.addEventListener("pointermove", (event) => {
    const box = cell.getBoundingClientRect();
    cell.style.setProperty("--x", `${event.clientX - box.left}px`);
    cell.style.setProperty("--y", `${event.clientY - box.top}px`);
  }));

  // The windows lean toward the pointer
  const stage = $(".stage");
  if (stage && !calm && matchMedia("(hover: hover)").matches) {
    const windows = $$(".pw", stage);
    $(".hero").addEventListener("pointermove", (event) => {
      const x = event.clientX / innerWidth - 0.5;
      const y = event.clientY / innerHeight - 0.5;
      windows.forEach((item, index) => {
        const depth = index === 1 ? 14 : 8;
        item.style.transform = `translate(${x * depth}px, ${y * depth}px) rotateY(${x * depth * 0.7}deg) rotateX(${-y * depth * 0.5}deg)`;
      });
    });
  }

  // A pointer works the calculator, the way a mouse controls the phone
  const pad = $("[data-calc]");
  if (pad && !calm) {
    const screen = pad.closest(".screen");
    const out = $("[data-out]", screen);
    const expression = $("[data-exp]", screen);
    const pointer = $(".cursor", screen);
    const sums = [["7", "×", "6", "="], ["1", "2", "+", "3", "0", "="], ["9", "8", "−", "5", "6", "="], ["4", "×", "9", "="]];
    const wait = (ms) => new Promise((done) => setTimeout(done, ms));
    const solve = (text) => {
      const [, a, op, b] = text.match(/^(\d+)([+−×])(\d+)$/);
      return String(op === "+" ? +a + +b : op === "−" ? a - b : a * b);
    };
    (async () => {
      for (let round = 0; ; round = (round + 1) % sums.length) {
        let typed = "";
        out.textContent = "0";
        expression.textContent = " ";
        for (const key of sums[round]) {
          const button = $$("span", pad).find((item) => item.textContent === key);
          const area = screen.getBoundingClientRect();
          const box = button.getBoundingClientRect();
          pointer.style.transform = `translate(${box.left - area.left + box.width / 2}px, ${box.top - area.top + box.height / 2}px)`;
          await wait(620);
          button.classList.add("hit");
          await wait(130);
          button.classList.remove("hit");
          if (key === "=") {
            expression.textContent = typed.replace(/([+−×])/, " $1 ");
            out.textContent = solve(typed);
          } else {
            typed += key;
            out.textContent = typed.replace(/([+−×])/, " $1 ");
          }
        }
        await wait(1900);
      }
    })();
  }

  // Finds this computer, so the right download can be offered first.
  const latest = "https://github.com/SahinurDEV/OpenVysor/releases/latest/download/";
  const downloads = {
    "windows-exe": ["Windows", "OpenVysor.exe", ".exe, portable"],
    "mac-arm-dmg": ["macOS (Apple Silicon)", latest + "OpenVysor-macos-arm64.dmg", ".dmg, preview build"],
    "mac-intel-dmg": ["macOS (Intel)", latest + "OpenVysor-macos-x64.dmg", ".dmg, preview build"],
    "linux-deb": ["Linux", latest + "OpenVysor-linux-x64.deb", ".deb, preview build"],
  };
  const agent = navigator.userAgent;
  const platform = /Android|iPhone|iPad/.test(agent) ? "phone" : /Mac/.test(agent) ? "macos" : /Linux|X11/.test(agent) ? "linux" : "windows";
  const macIsIntel = async () => {
    try {
      const hints = await navigator.userAgentData?.getHighEntropyValues(["architecture"]);
      if (hints?.architecture) return hints.architecture === "x86";
    } catch {}
    try {
      const gl = document.createElement("canvas").getContext("webgl");
      const info = gl.getExtension("WEBGL_debug_renderer_info");
      return /Intel/.test(gl.getParameter(info.UNMASKED_RENDERER_WEBGL));
    } catch { return false; }
  };
  const offer = (key) => {
    const [name, href, kind] = downloads[key];
    $$("[data-download]").forEach((link) => {
      link.href = href;
      link.toggleAttribute("download", true);
      $("span", link).textContent = `Download for ${name}`;
    });
    const note = $("[data-download-note]");
    if (note) note.innerHTML = `${kind}. Not your computer? <a href="#install">See all downloads</a>.`;
    $$(".dl").forEach((row) => row.classList.toggle("recommended", row.dataset.key === key));
  };
  if (platform === "phone") {
    const note = $("[data-download-note]");
    if (note) note.textContent = "OpenVysor runs on a computer. Open this page on your PC or Mac to download it.";
  } else if (platform === "macos") {
    offer("mac-arm-dmg");
    macIsIntel().then((intel) => { if (intel) offer("mac-intel-dmg"); });
  } else {
    offer(platform === "linux" ? "linux-deb" : "windows-exe");
  }

  // Tabs. The choice is shared by every tab set and kept between pages.
  const chooseTab = (name) => $$("[data-tabs]").forEach((group) => {
    $$("[role=tab]", group).forEach((tab) => {
      const on = tab.dataset.tab === name;
      tab.setAttribute("aria-selected", String(on));
      tab.tabIndex = on ? 0 : -1;
      const panel = document.getElementById(tab.getAttribute("aria-controls"));
      if (panel) panel.hidden = !on;
    });
  });
  $$("[data-tabs]").forEach((group) => {
    const tabs = $$("[role=tab]", group);
    tabs.forEach((tab, index) => {
      tab.addEventListener("click", () => { chooseTab(tab.dataset.tab); store.set("platform", tab.dataset.tab); });
      tab.addEventListener("keydown", (event) => {
        const step = event.key === "ArrowRight" ? 1 : event.key === "ArrowLeft" ? -1 : 0;
        if (!step) return;
        const next = tabs[(index + step + tabs.length) % tabs.length];
        chooseTab(next.dataset.tab);
        next.focus();
      });
    });
  });
  if ($("[data-tabs]")) chooseTab(store.get("platform") || (platform === "phone" ? "windows" : platform));

  // Copy buttons
  $$(".term").forEach((block) => {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "copy";
    button.textContent = "Copy";
    button.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText($("code", block).textContent);
        button.textContent = "Copied";
        button.classList.add("done");
      } catch {
        button.textContent = "Select it";
      }
      setTimeout(() => { button.textContent = "Copy"; button.classList.remove("done"); }, 1600);
    });
    block.append(button);
  });

  // Status filter
  const chips = $$(".chips button");
  chips.forEach((chip) => chip.addEventListener("click", () => {
    chips.forEach((item) => item.setAttribute("aria-pressed", String(item === chip)));
    $$(".matrix li").forEach((row) => { row.hidden = chip.dataset.filter !== "all" && row.dataset.state !== chip.dataset.filter; });
  }));

  // Docs navigation and search
  const sideLinks = $$(".side nav a");
  if (sideLinks.length) {
    const byId = new Map(sideLinks.map((link) => [link.getAttribute("href").slice(1), link]));
    const sections = $$(".doc section[id]");
    const mark = () => {
      let current = sections[0];
      for (const section of sections) if (section.getBoundingClientRect().top < 140) current = section;
      sideLinks.forEach((link) => link.classList.remove("active"));
      byId.get(current.id)?.classList.add("active");
    };
    addEventListener("scroll", mark, { passive: true });
    mark();

    const input = $(".search input");
    const empty = $(".no-hits");
    input.addEventListener("input", () => {
      const words = input.value.toLowerCase().split(/\s+/).filter(Boolean);
      let hits = 0;
      sideLinks.forEach((link) => {
        const text = (link.textContent + " " + document.getElementById(link.getAttribute("href").slice(1)).textContent).toLowerCase();
        const match = words.every((word) => text.includes(word));
        link.hidden = !match;
        hits += match;
      });
      $$(".side h5").forEach((heading) => {
        let next = heading.nextElementSibling;
        let any = false;
        while (next && next.tagName === "A") { any ||= !next.hidden; next = next.nextElementSibling; }
        heading.hidden = !any;
      });
      empty.hidden = hits > 0;
    });
    addEventListener("keydown", (event) => {
      if (event.key === "/" && !/INPUT|TEXTAREA/.test(document.activeElement.tagName)) {
        event.preventDefault();
        $(".side").classList.add("open");
        input.focus();
      }
    });
  }
})();
