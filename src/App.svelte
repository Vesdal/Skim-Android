<script lang="ts">
  import Titlebar from "./components/Titlebar.svelte";
  import Sidebar from "./components/Sidebar.svelte";
  import MessageList from "./components/MessageList.svelte";
  import ReadingPane from "./components/ReadingPane.svelte";
  import ComposeForm from "./components/ComposeForm.svelte";
  import AiAsk from "./components/AiAsk.svelte";
  import AiRecap from "./components/AiRecap.svelte";
  import CommandPalette from "./components/CommandPalette.svelte";
  import FolderEditor from "./components/FolderEditor.svelte";
  import FolderPicker from "./components/FolderPicker.svelte";
  import ShortcutsOverlay from "./components/ShortcutsOverlay.svelte";
  import Onboarding from "./components/onboarding/Onboarding.svelte";
  import { untrack } from "svelte";
  import type { ChatSession } from "./lib/ai-chat";
  import { api, reportError, type Citation } from "./lib/api";
  import { bulkAct, bulkMove } from "./lib/bulk";
  import { setLocale, t } from "./lib/i18n/index.svelte";
  import { ai } from "./lib/stores/ai.svelte";
  import { aiSessions } from "./lib/stores/aiSession.svelte";
  import { mail, rowKey, UNIFIED } from "./lib/stores/mail.svelte";
  import { palette } from "./lib/stores/palette.svelte";
  import { ui } from "./lib/stores/ui.svelte";
  import { updater } from "./lib/stores/update.svelte";
  import type { Account, Draft, ThreadRow } from "./lib/types";

  /** How long the boot is given to answer before the shell is drawn anyway.
   *  Long enough that a healthy start never flashes an empty frame, short
   *  enough that a stalled one is still a window the user can use. */
  const BOOT_PATIENCE_MS = 2000;

  let ready = $state(false);

  const inTauri = "__TAURI_INTERNALS__" in window;
  const onAndroid =
    typeof navigator !== "undefined" && navigator.userAgent.includes("Android");
  // Onboarding is only the right screen once we know there is nothing to show.
  // The shell is now drawn before `boot()` has answered, so an empty account
  // list can simply mean the answer hasn't arrived yet — and a returning user
  // must never be greeted by the welcome screen.
  const needsOnboarding = $derived(mail.accounts.length === 0 && (mail.booted || !inTauri));

  // Opening a message dismisses the recap panel.
  $effect(() => {
    if (mail.selectedThreadId !== null && ui.recapOpen) ui.closeRecap();
  });

  // Mobile drawer follows navigation: picking a folder or a thread closes it.
  // NB: drawerOpen is read untracked — otherwise opening the drawer would
  // retrigger this effect and immediately close it again.
  $effect(() => {
    void mail.selectedFolderId;
    void mail.selectedThreadId;
    untrack(() => {
      if (ui.drawerOpen) ui.closeDrawer();
    });
  });

  // Stay in sync without manual reloads: re-sync when coming back to the app
  // (the OS may have killed sockets while we were away). Throttled — IMAP
  // IDLE already pushes live mail while open, and each sync skips untouched
  // folders server-side, so this costs no background battery: nothing runs
  // while the app is closed.
  $effect(() => {
    let lastFgSync = 0;
    const onVisible = () => {
      if (document.visibilityState !== "visible") return;
      const now = Date.now();
      if (now - lastFgSync < 30_000) return;
      lastFgSync = now;
      if (mail.accounts.length > 0) void mail.syncNow();
    };
    document.addEventListener("visibilitychange", onVisible);
    window.addEventListener("pageshow", onVisible);
    return () => {
      document.removeEventListener("visibilitychange", onVisible);
      window.removeEventListener("pageshow", onVisible);
    };
  });

  // Android system back button (MainActivity asks `window.__skimGoBack` before
  // falling through to WebView history / exit). Topmost surface first; the
  // reader slot (thread / recap / draft editor) steps back to the list.
  // Synchronous on purpose — the native side needs an immediate answer.
  $effect(() => {
    const goBack = () => {
      if (palette.open) {
        palette.hide();
        return true;
      }
      if (ui.shortcutsOpen) {
        ui.closeShortcuts();
        return true;
      }
      if (ui.movePicker !== null) {
        ui.closeMove();
        return true;
      }
      if (ui.folderEditor !== null) {
        ui.closeFolderEditor();
        return true;
      }
      if (ui.settingsOpen) {
        ui.closeSettings();
        return true;
      }
      if (expandedAsk) {
        aiSessions.close(expandedAsk);
        return true;
      }
      if (inlineAsk?.open) {
        aiSessions.close(inlineAsk);
        return true;
      }
      if (ui.drawerOpen) {
        ui.closeDrawer();
        return true;
      }
      if (mail.selecting) {
        mail.clearSelection();
        return true;
      }
      if (ui.recapOpen) {
        ui.closeRecap();
        return true;
      }
      if (mail.selectedThreadId !== null || draftEditorId !== null) {
        mail.selectedThreadId = null;
        mail.selectedMessageId = null;
        return true;
      }
      // Inside a folder: step back to its inbox instead of leaving the app.
      const inbox = mail.folders.find((f) => f.role === "inbox");
      if (inbox && mail.selectedFolderId !== inbox.id) {
        void mail.selectFolder(inbox.id);
        return true;
      }
      return false;
    };
    (window as unknown as { __skimGoBack?: () => boolean }).__skimGoBack = goBack;
    return () => {
      delete (window as unknown as { __skimGoBack?: () => boolean }).__skimGoBack;
    };
  });

  // Haptics, centrally: every touch tap on a button ticks once, on press.
  // One delegated listener covers the whole app (present and future buttons);
  // desktops have no vibrate API so this is a silent no-op there.
  $effect(() => {
    if (
      typeof window === "undefined" ||
      typeof window.matchMedia !== "function" ||
      !window.matchMedia("(pointer: coarse)").matches
    )
      return;
    const onDown = (e: PointerEvent) => {
      if (e.pointerType !== "touch") return;
      const el = e.target as HTMLElement | null;
      if (el?.closest?.("button")) {
        try {
          navigator.vibrate?.(10);
        } catch {
          // haptics are best-effort
        }
      }
    };
    document.addEventListener("pointerdown", onDown, { passive: true });
    return () => {
      document.removeEventListener("pointerdown", onDown);
    };
  });

  // ---- AI recap ----
  // The digest is a chat session like any other, so it survives the panel
  // closing and can be popped into a window of its own. The panel only decides
  // which folder is being digested.
  const recap = $derived(aiSessions.recap);

  // The chat about the open email, while an inline surface is showing it — the
  // same session the reading pane docks. Scoped to that email on purpose: a
  // chat left open on a message the user has since navigated away from is not
  // on screen, and must not go on claiming Escape.
  const inlineAsk = $derived.by(() => {
    const session = aiSessions.askFor(ui.openMessageId);
    return session?.open ? session : undefined;
  });
  // Expanded, it is drawn here — over the panes it covers.
  const expandedAsk = $derived(inlineAsk?.expanded ? inlineAsk : undefined);
  $effect(() => {
    // The panel closing ends the digest — whichever way it was closed (✕, Esc,
    // opening a message). A digest that moved into a window is not this panel's
    // any more, so `aiSessions.recap` no longer sees it and it keeps streaming.
    if (!ui.recapOpen) {
      const stale = untrack(() => aiSessions.recap);
      if (stale) aiSessions.drop(stale);
      return;
    }
    const folderId = mail.selectedFolderId;
    if (folderId === null) return;
    // A fresh digest per folder — but never a second scan of a folder that
    // already has one, even if that one is off in a window. Untracked so this
    // reacts to the folder alone, not to the session it starts.
    if (!untrack(() => aiSessions.recapFor(folderId))) aiSessions.startRecap(folderId);
  });

  function closeRecap() {
    ui.closeRecap();
  }

  function popOutRecap(session: ChatSession) {
    void aiSessions.detach(session);
    // detach() marks the session synchronously, so the panel closing here can't
    // be mistaken for the user dismissing the digest.
    ui.closeRecap();
  }

  async function openRecapCitation(c: Citation) {
    // openLocation maps the real folder onto the current scope — in the
    // unified view that's the virtual counterpart, not the folder itself.
    await mail.openLocation(c.folderId, c.threadId, c.messageId);
    ui.closeRecap();
  }

  // ---- In-pane draft editor (Drafts folder) ----
  // Selecting a draft opens the compose surface in the reading pane instead of
  // the read-only view. `draftEditorId` is the local draft to edit; `draftOrigin`
  // the Drafts-folder message it mirrors (needed to remove it on discard).
  let draftEditorId = $state<number | null>(null);
  let draftOrigin = $state<number | null>(null);
  let draftResolveToken = 0;

  // Mobile single-pane: the reader slot (thread, recap or draft editor)
  // takes the whole screen; the list shows otherwise.
  const showReader = $derived(
    mail.selectedThreadId !== null || ui.recapOpen || draftEditorId !== null,
  );

  $effect(() => {
    const folder = mail.selectedFolder;
    const threadId = mail.selectedThreadId;
    const messageId = mail.selectedMessageId;
    if (folder?.role !== "drafts" || threadId === null) {
      draftEditorId = null;
      draftOrigin = null;
      return;
    }
    const token = ++draftResolveToken;
    void (async () => {
      try {
        // Grouped rows carry no messageId — resolve the draft message from the
        // thread (its latest message).
        let msgId = messageId;
        if (msgId == null) {
          const detail = await api.getThread(threadId);
          msgId = detail.messages[detail.messages.length - 1]?.id ?? null;
        }
        if (msgId == null) return;
        const draft = await api.editDraft(msgId);
        if (token !== draftResolveToken) return; // selection moved on
        draftOrigin = msgId;
        draftEditorId = draft.id;
      } catch {
        if (token === draftResolveToken) {
          draftEditorId = null;
          draftOrigin = null;
        }
      }
    })();
  });

  function draftPreview(body: string): string {
    return (body.split(/\r?\n/).find((l) => l.trim()) ?? "").slice(0, 200);
  }

  /** Reflect a live autosave in the Drafts list row. */
  function onDraftLocalSave(d: Draft) {
    if (mail.selectedThreadId !== null) {
      mail.patchThreadRow(mail.selectedThreadId, {
        subject: d.subject,
        snippet: draftPreview(d.body),
      });
    }
  }

  function onDraftSent() {
    // The backend removes the server copy from Drafts; drop the row now.
    if (mail.selectedThreadId !== null) mail.removeThreadFromList(mail.selectedThreadId);
    draftEditorId = null;
    draftOrigin = null;
  }

  function onDraftDiscarded() {
    // The form deleted the local draft; remove the server copy too.
    if (draftOrigin !== null) void api.deleteMessages([draftOrigin]);
    if (mail.selectedThreadId !== null) mail.removeThreadFromList(mail.selectedThreadId);
    draftEditorId = null;
    draftOrigin = null;
  }

  $effect(() => {
    void (async () => {
      if (inTauri) {
        let settings: Record<string, string> = {};
        try {
          settings = await api.getSettings();
          if (settings.locale) await setLocale(settings.locale as never);
          // Apply the stored theme (migrating legacy values); persist the
          // normalized string back once if migration changed it. Nothing is
          // written when no theme was stored yet, so a fresh install keeps
          // following the OS lightness on every boot until the user picks one.
          const normalized = ui.hydrate(settings.theme);
          if (settings.theme && settings.theme !== normalized)
            void api.setSetting("theme", normalized).catch(() => {});
          if (settings.sidebar_collapsed) ui.setSidebarCollapsed(settings.sidebar_collapsed === "on");
        } catch {
          // settings are best-effort at boot
        }
        // Desktop-only: mobile updates come from reinstalling the APK, never
        // from the Windows updater asking inside the app.
        if (!onAndroid) updater.init(settings);
        // The shell is drawn whatever the mailbox is doing. `boot()` reads a
        // database the startup sync is busy filling, so it can be slow, and if
        // it throws there is nothing later that would ever set `ready` — either
        // way the window would show a titlebar over nothing, with no way to
        // tell a busy app from a dead one. Wait a beat for the usual instant
        // answer, then draw regardless; the mail arrives into a live window.
        const booting = mail.boot().catch((e: unknown) => {
          reportError("mail.boot", e);
          mail.noteError(t("ops.failed"));
        });
        await Promise.race([booting, new Promise((done) => setTimeout(done, BOOT_PATIENCE_MS))]);
        void ai.refresh();
      }
      ready = true;
    })();
  });

  function onboarded(account: Account) {
    void mail.accountAdded(account);
    void ai.refresh();
  }

  function isTyping(): boolean {
    const el = document.activeElement;
    return (
      el instanceof HTMLInputElement ||
      el instanceof HTMLTextAreaElement ||
      (el instanceof HTMLElement && el.isContentEditable)
    );
  }

  function moveSelection(delta: number) {
    const threads = mail.threads;
    if (threads.length === 0) return;
    // Rows are keyed by thread in grouped mode, by message in flat mode (where
    // several rows can share a thread id).
    const index = mail.groupThreads
      ? threads.findIndex((t) => t.id === mail.selectedThreadId)
      : threads.findIndex((t) => t.messageId === mail.selectedMessageId);
    const next = index === -1 ? 0 : Math.max(0, Math.min(threads.length - 1, index + delta));
    const row = threads[next];
    mail.selectedThreadId = row.id;
    mail.selectedMessageId = row.messageId ?? null;
  }

  /** The row the list is highlighting, in either grouping mode. */
  function highlightedRow(): ThreadRow | null {
    return (
      mail.threads.find((t) =>
        mail.groupThreads
          ? t.id === mail.selectedThreadId
          : t.messageId === mail.selectedMessageId,
      ) ?? null
    );
  }

  async function actOnSelected(action: "archive" | "delete" | "spam" | "star" | "unread") {
    // Ticked rows are what the action keys act on; the highlighted row is the
    // target only when nothing is ticked. Star has no bulk form — it is not a
    // batch verb — so it stays on the highlighted row either way.
    if (mail.selecting && action !== "star") {
      void bulkAct(action === "unread" ? "read" : action);
      return;
    }
    const thread = mail.selectedThread;
    if (!thread) return;
    const ids = await api.threadMessageIds(thread.id);
    if (ids.length === 0) return;
    switch (action) {
      case "archive":
        mail.removeThreadFromList(thread.id);
        void api.archiveMessages(ids);
        break;
      case "delete":
        mail.removeThreadFromList(thread.id);
        void api.deleteMessages(ids);
        break;
      case "spam":
        mail.removeThreadFromList(thread.id);
        void api.reportSpam(ids);
        break;
      case "star":
        mail.patchThreadRow(thread.id, { isStarred: !thread.isStarred });
        void api.setStarred(ids, !thread.isStarred);
        break;
      case "unread": {
        const next = !thread.isRead;
        mail.patchThreadRow(thread.id, { isRead: next });
        void api.markRead(ids, next);
        break;
      }
    }
  }

  /** Open the folder picker for the highlighted thread — works with the reading
   *  pane closed, which is why the picker lives here and not in it. */
  async function openMoveForSelected() {
    if (mail.selecting) {
      void bulkMove();
      return;
    }
    const thread = mail.selectedThread;
    if (!thread) return;
    const ids = await api.threadMessageIds(thread.id);
    if (ids.length === 0) return;
    ui.openMove({ rowKeys: mail.rowKeysForThread(thread.id), messageIds: ids });
  }

  async function replyToSelected(mode: "reply" | "reply_all" | "forward" = "reply") {
    const thread = mail.selectedThread;
    if (!thread) return;
    const detail = await api.getThread(thread.id);
    const latest = detail.messages[detail.messages.length - 1];
    const draft = await api.getReplyTemplate(latest.id, mode);
    await api.openComposeWindow(draft.id);
  }

  async function composeNew() {
    const draft = await api.createDraft(await mail.composeAccountId());
    await api.openComposeWindow(draft.id);
  }

  function onKeydown(e: KeyboardEvent) {
    if (mail.accounts.length === 0) return;

    // Letter shortcuts match the physical key (e.code), not the produced
    // character (e.key): in a Cyrillic (or any non-Latin) layout the K key
    // emits "л", not "k", so an e.key check would only work in a US layout.
    // Ctrl/Cmd+K opens the palette (idempotent — no-op when already open); the
    // chat views have their own chords (Ctrl+E, Ctrl+Shift+E).
    if ((e.ctrlKey || e.metaKey) && e.code === "KeyK") {
      e.preventDefault();
      palette.show();
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.code === "KeyN") {
      e.preventDefault();
      void composeNew();
      return;
    }
    // Ctrl+1 is "All inboxes", Ctrl+2..9 the Nth mailbox — matching the
    // switcher's order (only when several accounts exist).
    if ((e.ctrlKey || e.metaKey) && mail.accounts.length > 1 && /^Digit[1-9]$/.test(e.code)) {
      const n = Number(e.code.slice(5));
      const target = n === 1 ? UNIFIED : mail.accounts[n - 2]?.id;
      if (target) {
        e.preventDefault();
        void mail.switchAccount(target);
      }
      return;
    }
    // Ctrl+A ticks every loaded row. It has to be handled before the guard
    // below (which drops anything with Ctrl held), so it repeats the same
    // conditions: inside the composer or any dialog this must stay the
    // browser's own "select all text".
    if (
      (e.ctrlKey || e.metaKey) &&
      e.code === "KeyA" &&
      !isTyping() &&
      !palette.open &&
      !ui.shortcutsOpen &&
      ui.movePicker === null &&
      ui.folderEditor === null &&
      expandedAsk === undefined &&
      mail.threads.length > 0
    ) {
      e.preventDefault();
      mail.selectAllLoaded();
      return;
    }
    // Escape leaves the in-pane draft editor (its teardown writes edits back),
    // even while a field is focused — handle it before the typing guard.
    if (e.key === "Escape" && draftEditorId !== null && mail.selectedFolder?.role === "drafts") {
      e.preventDefault();
      mail.selectedThreadId = null;
      return;
    }
    // While the chat about an email is up, Escape is its own — it collapses the
    // chat, then closes it. Left to run, this would clear the thread selection
    // out from under it instead, which is the opposite of what Escape looks
    // like it should do there.
    if (e.key === "Escape" && inlineAsk) return;
    if (
      palette.open ||
      ui.shortcutsOpen ||
      ui.movePicker !== null ||
      ui.folderEditor !== null ||
      // The expanded chat covers the list: don't archive or move what is behind
      // it just because the focus has left its input.
      expandedAsk !== undefined ||
      isTyping() ||
      e.ctrlKey ||
      e.metaKey ||
      e.altKey
    )
      return;
    // The in-pane draft editor owns the keyboard — don't let list shortcuts
    // (archive/reply/star…) act on the draft being edited.
    if (mail.selectedFolder?.role === "drafts" && draftEditorId !== null) return;

    switch (e.code) {
      case "KeyJ":
        moveSelection(1);
        return;
      case "KeyK":
        moveSelection(-1);
        return;
      case "KeyE":
        void actOnSelected("archive");
        return;
      case "KeyS":
        void actOnSelected("star");
        return;
      case "KeyU":
        void actOnSelected("unread");
        return;
      case "KeyV":
        void openMoveForSelected();
        return;
      case "KeyX": {
        // Tick the highlighted row — the keyboard's way into a selection.
        const row = highlightedRow();
        if (row) mail.toggleRow(rowKey(row));
        return;
      }
      case "KeyR":
        void replyToSelected("reply");
        return;
      case "KeyA":
        void replyToSelected("reply_all");
        return;
      case "KeyF":
        void replyToSelected("forward");
        return;
      case "KeyQ":
        // preventDefault so the "q" that opened the dock isn't also typed into
        // the Ask input we're about to focus.
        if (ai.keyPresent) {
          e.preventDefault();
          ui.readingAi?.ask();
        }
        return;
      case "KeyT":
        // Toggles the translation, whether or not the bar offered one — the
        // language guess decides what to suggest, never what is possible.
        if (ai.keyPresent) {
          e.preventDefault();
          ui.readingAi?.translate();
        }
        return;
    }

    // Symbol / navigation keys are layout-independent enough to match on e.key.
    switch (e.key) {
      case "/":
        e.preventDefault();
        palette.show();
        break;
      case "?":
        e.preventDefault();
        ui.openShortcuts();
        break;
      case ".":
        e.preventDefault();
        ui.toggleSidebar();
        break;
      case "#":
      case "Delete":
        void actOnSelected("delete");
        break;
      case "!":
        void actOnSelected("spam");
        break;
      case "Escape":
        // A ticked selection is the most recent thing the user built, so it is
        // the first thing Escape takes back.
        if (mail.selecting) mail.clearSelection();
        else if (ui.recapOpen) ui.closeRecap();
        else mail.selectedThreadId = null;
        break;
    }
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="app">
  <Titlebar />
  {#if ready}
    {#if !needsOnboarding}
      <main
        class="panes"
        class:has-thread={showReader}
        class:drawer-open={ui.drawerOpen}
      >
        <Sidebar />
        {#if ui.drawerOpen}
          <button
            class="drawer-scrim"
            aria-label={t("a11y.close")}
            onclick={() => ui.closeDrawer()}
          ></button>
        {/if}
        <MessageList />
        {#if mail.selectedFolder?.role === "drafts" && draftEditorId !== null}
          {#key draftEditorId}
            <ComposeForm
              draftId={draftEditorId}
              onSent={onDraftSent}
              onDiscarded={onDraftDiscarded}
              onLocalSave={onDraftLocalSave}
            />
          {/key}
        {:else if ui.recapOpen && mail.selectedThreadId === null}
          <!-- The session is started by the effect above, a tick after the panel
               opens; until then the pane stays blank rather than flashing. -->
          {#if recap}
            <AiRecap
              session={recap}
              onsend={(q) => aiSessions.send(recap, q)}
              oncitation={openRecapCitation}
              onclose={closeRecap}
              onpopout={() => popOutRecap(recap)}
            />
          {/if}
        {:else}
          <ReadingPane />
        {/if}
      </main>
      {#if expandedAsk}
        <!-- The chat about an email, given the whole window. Drawn here and not
             in the reading pane because that pane is one of the things it
             covers; the dock and this are the same session, so switching
             between them costs nothing. -->
        <div class="ask-full">
          <AiAsk
            view="full"
            session={expandedAsk}
            onsend={(q) => aiSessions.send(expandedAsk, q)}
            onclose={() => aiSessions.close(expandedAsk)}
            onpopout={() => void aiSessions.detach(expandedAsk)}
            ontoggleexpand={() => aiSessions.toggleExpand(expandedAsk)}
          />
        </div>
      {/if}
      <CommandPalette />
      <FolderPicker />
      <FolderEditor />
      {#if ui.shortcutsOpen}
        <ShortcutsOverlay />
      {/if}
    {:else}
      <Onboarding oncomplete={onboarded} />
    {/if}
  {/if}
</div>

<style>
  .app {
    height: 100%;
    height: 100dvh;
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    /* Edge-to-edge: consuma le safe-area (status bar, punch-hole, gesture
       bar) dentro l'app invece di lasciare bande nere o contenuti coperti. */
    padding-top: env(safe-area-inset-top, 0px);
    padding-right: env(safe-area-inset-right, 0px);
    padding-bottom: env(safe-area-inset-bottom, 0px);
    padding-left: env(safe-area-inset-left, 0px);
  }
  .panes {
    flex: 1;
    display: flex;
    min-height: 0;
  }
  /* Everything below the titlebar — which stays reachable, so the window can
     still be moved, minimised and closed while the chat is up. No scrim: the
     panel is opaque, and there is nothing left showing to dim. */
  .ask-full {
    position: fixed;
    inset: var(--titlebar-h) 0 0 0;
    z-index: 100;
    display: flex;
    background: var(--bg);
  }
  .drawer-scrim {
    display: none;
  }

  /* ---- Medium screens (landscape phones, small tablets, narrow windows) ----
     La sidebar diventa un drawer anche qui: a 761–1100px tre pannelli fissi
     (224 + 372 + lettore) non ci stanno e il lettore resterebbe schiacciato.
     Lista + lettore restano affiancati, la lista si restringe. */
  @media (max-width: 1100px) {
    .panes > :global(.sidebar) {
      position: fixed;
      top: calc(var(--titlebar-h) + env(safe-area-inset-top, 0px));
      bottom: env(safe-area-inset-bottom, 0px);
      left: env(safe-area-inset-left, 0px);
      width: min(300px, 84vw);
      z-index: 60;
      background: var(--bg);
      border-right: 1px solid var(--hairline-strong);
      box-shadow: var(--shadow-pop);
      transform: translateX(-105%);
      visibility: hidden;
      transition:
        transform 0.25s ease-out,
        visibility 0.25s;
    }
    .panes.drawer-open > :global(.sidebar) {
      transform: translateX(0);
      visibility: visible;
    }
    .drawer-scrim {
      position: fixed;
      inset: calc(var(--titlebar-h) + env(safe-area-inset-top, 0px)) 0 0 0;
      z-index: 55;
      background: rgba(0, 0, 0, 0.35);
      opacity: 0;
      visibility: hidden;
      pointer-events: none;
      transition:
        opacity 0.22s ease-out,
        visibility 0.22s;
    }
    .panes.drawer-open .drawer-scrim {
      display: block;
      opacity: 1;
      visibility: visible;
      pointer-events: auto;
    }
    .panes > :global(.list) {
      width: min(340px, 44vw);
      flex-shrink: 0;
    }
  }

  /* ---- Mobile single-pane (phones, narrow windows) ----
     One pane at a time at full width: the folder list, or the reader slot
     (thread / recap / draft editor). Type stays at desktop sizes — nothing
     shrinks to fit. Motion is transform/opacity only, and off entirely under
     reduced-motion. */
  @media (max-width: 760px) {
    .panes > :global(.list),
    .panes > :global(section:last-child),
    .panes > :global(.compose-form) {
      width: 100%;
      flex: 1 1 auto;
      border-right: none;
    }
    /* Reader slot children: ReadingPane, ComposeForm, AiRecap section. */
    .panes:not(.has-thread) > :global(section:last-child),
    .panes:not(.has-thread) > :global(.compose-form) {
      display: none;
    }
    .panes.has-thread > :global(.list) {
      display: none;
    }
    .panes:not(.has-thread) > :global(.list) {
      animation: pane-in 0.22s ease-out;
    }
    .panes.has-thread > :global(section:last-child),
    .panes.has-thread > :global(.compose-form) {
      animation: reader-in 0.25s ease-out;
    }
  }
  @keyframes pane-in {
    from {
      opacity: 0;
    }
    to {
      opacity: 1;
    }
  }
  @keyframes reader-in {
    from {
      transform: translateX(28px);
      opacity: 0;
    }
    to {
      transform: translateX(0);
      opacity: 1;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .panes > :global(.sidebar),
    .drawer-scrim {
      transition: none;
    }
    .panes:not(.has-thread) > :global(.list),
    .panes.has-thread > :global(section:last-child),
    .panes.has-thread > :global(.compose-form) {
      animation: none;
    }
  }
  /* No window chrome on touch devices: the OS owns the window. La titlebar
     desktop (drag, min/max/close) su Android e' spazio sprecato e coprirebbe
     la status bar in edge-to-edge: la lista ha gia' l'hamburger e il lettore
     il back. */
  @media (pointer: coarse) {
    .app > :global(.titlebar) {
      display: none;
    }
    .ask-full {
      inset: env(safe-area-inset-top, 0px) env(safe-area-inset-right, 0px) env(safe-area-inset-bottom, 0px) env(safe-area-inset-left, 0px);
    }
    /* Senza titlebar il drawer parte dalla safe-area in alto. */
    .panes > :global(.sidebar) {
      top: env(safe-area-inset-top, 0px);
    }
    .drawer-scrim {
      inset: env(safe-area-inset-top, 0px) 0 0 0;
    }
  }
</style>
