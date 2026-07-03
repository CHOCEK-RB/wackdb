<script lang="ts">
  import * as Tabs from "$lib/components/ui/tabs";
  import { Button } from "$lib/components/ui/button";
  import * as Card from "$lib/components/ui/card";
  import { page } from "$app/state";
  import { goto } from "$app/navigation";
  import { toggleMode } from "mode-watcher";
  import { Sun, Moon, Database, Activity, RefreshCw } from "@lucide/svelte";
  
  import type { 
      BufferPoolStateDto, 
      PageDto, 
      BTreePageDto, 
      FrameDumpDto, 
      TableMetadataDto 
  } from "$lib/types";

  import CatalogView from "$lib/components/CatalogView.svelte";
  import BufferPoolView from "$lib/components/BufferPoolView.svelte";
  import InspectorView from "$lib/components/InspectorView.svelte";

  let bufferData = $state<BufferPoolStateDto>({ hits: 0, misses: 0, hit_rate: 0, frames: [] });
  let isPolling = $state(false);
  
  // State derived from URL query parameters (enables back button support)
  let activeTab = $derived(page.url.searchParams.get("tab") || "buffer_pool");
  let inspectorMode = $derived(page.url.searchParams.get("mode") as "frame" | "heap_page" | "btree_page" | null);
  
  let selectedFrameId = $derived(
      page.url.searchParams.has("frame") ? parseInt(page.url.searchParams.get("frame")!) : null
  );
  
  let selectedPageId = $derived(
      page.url.searchParams.has("file") && page.url.searchParams.has("page")
          ? { file: parseInt(page.url.searchParams.get("file")!), page: parseInt(page.url.searchParams.get("page")!) }
          : null
  );
  
  // Inspector Data state
  let inspectedPage = $state<PageDto | null>(null);
  let inspectedBTreePage = $state<BTreePageDto | null>(null);
  let inspectedFrameDump = $state<FrameDumpDto | null>(null);
  
  // Catalog state
  let catalogTables = $state<TableMetadataDto[]>([]);
  let selectedTable = $state<TableMetadataDto | null>(null);
  let tableHeapNumPages = $state<number>(0);
  let tableIndexNumPages = $state<number>(0);

  // React to URL changes and fetch the necessary data
  $effect(() => {
      if (inspectorMode === "frame" && selectedFrameId !== null) {
          fetchPageContentByFrame(selectedFrameId);
      } else if (inspectorMode === "heap_page" && selectedPageId !== null) {
          fetchPageContentByFile(selectedPageId.file, selectedPageId.page);
      } else if (inspectorMode === "btree_page" && selectedPageId !== null) {
          fetchBTreePageContentByFile(selectedPageId.file, selectedPageId.page);
      }
  });

  async function fetchBufferState() {
    try {
      const res = await fetch("http://127.0.0.1:3000/api/buffer_pool");
      if (res.ok) {
        bufferData = await res.json();
      }
      fetchCatalog();
    } catch (err) {
      console.error("Error fetching buffer state:", err);
    }
  }

  async function fetchCatalog() {
      try {
          const res = await fetch("http://127.0.0.1:3000/api/catalog/tables");
          if (res.ok) catalogTables = await res.json();
      } catch (err) {}
  }

  async function selectCatalogTable(table: TableMetadataDto) {
      selectedTable = table;
      // Fetch heap pages count
      try {
          const res = await fetch(`http://127.0.0.1:3000/api/catalog/table/${table.name}/pages`);
          if (res.ok) tableHeapNumPages = await res.json();
      } catch (err) {}
      
      // Fetch index pages count via the optimized backend endpoint
      try {
          const res = await fetch(`http://127.0.0.1:3000/api/catalog/table/${table.name}/index_pages`);
          if (res.ok) tableIndexNumPages = await res.json();
      } catch (err) {}
  }

  async function fetchPageContentByFrame(frameId: number) {
    try {
      const res = await fetch(`http://127.0.0.1:3000/api/frame/${frameId}`);
      if (res.ok) {
        inspectedFrameDump = await res.json();
      } else {
        inspectedFrameDump = null;
      }
    } catch (err) {
      console.error("Error fetching frame:", err);
      inspectedFrameDump = null;
    }
  }
  
  async function fetchPageContentByFile(fileId: number, pageNum: number) {
    try {
      const res = await fetch(`http://127.0.0.1:3000/api/page/${fileId}/${pageNum}`);
      if (res.ok) {
        inspectedPage = await res.json();
      } else {
        inspectedPage = null;
      }
    } catch (err) {
      console.error("Error fetching page by file:", err);
      inspectedPage = null;
    }
  }

  async function fetchBTreePageContentByFile(fileId: number, pageNum: number) {
    try {
      const res = await fetch(`http://127.0.0.1:3000/api/btree_page/${fileId}/${pageNum}`);
      if (res.ok) {
        inspectedBTreePage = await res.json();
      } else {
        inspectedBTreePage = null;
      }
    } catch (err) {
      console.error("Error fetching btree page by file:", err);
      inspectedBTreePage = null;
    }
  }

  function viewPageByFrame(frameId: number) {
      const url = new URL(page.url);
      url.searchParams.set("tab", "pages");
      url.searchParams.set("mode", "frame");
      url.searchParams.set("frame", frameId.toString());
      url.searchParams.delete("file");
      url.searchParams.delete("page");
      goto(url.toString());
  }
  
  function viewHeapPageByFile(fileId: number, pageNum: number) {
      const url = new URL(page.url);
      url.searchParams.set("tab", "pages");
      url.searchParams.set("mode", "heap_page");
      url.searchParams.set("file", fileId.toString());
      url.searchParams.set("page", pageNum.toString());
      url.searchParams.delete("frame");
      goto(url.toString());
  }
  
  function viewBTreePageByFile(fileId: number, pageNum: number) {
      const url = new URL(page.url);
      url.searchParams.set("tab", "pages");
      url.searchParams.set("mode", "btree_page");
      url.searchParams.set("file", fileId.toString());
      url.searchParams.set("page", pageNum.toString());
      url.searchParams.delete("frame");
      goto(url.toString());
  }

  function togglePolling() {
    isPolling = !isPolling;
  }

  $effect(() => {
    if (!isPolling) return;
    fetchBufferState(); // Initial fetch
    const interval = setInterval(fetchBufferState, 1000);
    return () => { if (interval) clearInterval(interval); };
  });

  function handleTabChange(value: string) {
      const url = new URL(page.url);
      url.searchParams.set("tab", value);
      goto(url.toString());
  }
</script>

<div class="min-h-screen bg-background text-foreground selection:bg-primary selection:text-primary-foreground transition-colors duration-300">
  <div class="p-8 max-w-7xl mx-auto font-sans relative">
    
    <!-- Ambient Background Glow -->
    <div class="absolute top-0 left-1/2 -translate-x-1/2 w-[800px] h-[300px] opacity-20 pointer-events-none blur-[100px] bg-gradient-to-r from-primary via-blue-500 to-purple-500 rounded-full dark:opacity-30"></div>

    <div class="flex justify-between items-center mb-8 relative z-10 p-4 border rounded-2xl bg-card/50 backdrop-blur-md shadow-sm">
      <div class="flex items-center gap-3">
        <div class="p-3 bg-primary/10 text-primary rounded-xl ring-1 ring-primary/20">
          <Database class="w-6 h-6" />
        </div>
        <h1 class="text-3xl font-extrabold tracking-tight bg-clip-text text-transparent bg-gradient-to-r from-foreground to-foreground/70">
          WackDB <span class="font-light text-primary">Visualizer</span>
        </h1>
      </div>
      
      <div class="flex items-center gap-4">
        <div class="flex items-center gap-2 bg-secondary/50 px-3 py-1.5 rounded-full ring-1 ring-border">
          <Activity class="w-4 h-4 {isPolling ? 'text-green-500 animate-pulse' : 'text-muted-foreground'}" />
          <span class="text-sm font-medium {isPolling ? 'text-green-600 dark:text-green-400' : 'text-muted-foreground'}">
            {isPolling ? "Live Sync Active" : "Live Sync Paused"}
          </span>
        </div>
        <Button variant={isPolling ? "default" : "outline"} size="sm" onclick={togglePolling} class="gap-2 transition-all hover:scale-105 active:scale-95">
          <RefreshCw class="w-4 h-4 {isPolling ? 'animate-spin' : ''}" />
          {isPolling ? "Stop" : "Auto-Refresh"}
        </Button>
        <div class="w-px h-6 bg-border mx-1"></div>
        <Button variant="ghost" size="icon" onclick={toggleMode} class="rounded-full hover:bg-secondary transition-colors" title="Toggle theme">
          <Sun class="h-[1.2rem] w-[1.2rem] rotate-0 scale-100 transition-all dark:-rotate-90 dark:scale-0" />
          <Moon class="absolute h-[1.2rem] w-[1.2rem] rotate-90 scale-0 transition-all dark:rotate-0 dark:scale-100" />
          <span class="sr-only">Toggle theme</span>
        </Button>
      </div>
    </div>

  <Tabs.Root value={activeTab} onValueChange={handleTabChange} class="w-full">
    <Tabs.List class="grid w-full grid-cols-4 mb-8">
      <Tabs.Trigger value="tables">Catalog & Tables</Tabs.Trigger>
      <Tabs.Trigger value="buffer_pool">Buffer Pool</Tabs.Trigger>
      <Tabs.Trigger value="btree">B+ Tree (WIP)</Tabs.Trigger>
      <Tabs.Trigger value="pages">Page Inspector</Tabs.Trigger>
    </Tabs.List>

    <Tabs.Content value="tables" class="mt-4">
        <CatalogView 
            catalogTables={catalogTables}
            selectedTable={selectedTable}
            tableHeapNumPages={tableHeapNumPages}
            tableIndexNumPages={tableIndexNumPages}
            onSelectTable={selectCatalogTable}
            onViewHeapPage={viewHeapPageByFile}
            onViewBTreePage={viewBTreePageByFile}
        />
    </Tabs.Content>

    <Tabs.Content value="buffer_pool" class="mt-4">
        <BufferPoolView 
            bufferData={bufferData} 
            onViewFrame={viewPageByFrame} 
        />
    </Tabs.Content>
    
    <Tabs.Content value="btree" class="mt-4">
      <Card.Root>
        <Card.Header>
          <Card.Title>B+ Tree Visualizer</Card.Title>
          <Card.Description>Navigate the B+ Tree index (To be implemented).</Card.Description>
        </Card.Header>
      </Card.Root>
    </Tabs.Content>

    <Tabs.Content value="pages" class="mt-4">
        <InspectorView 
            inspectorMode={inspectorMode}
            selectedFrameId={selectedFrameId}
            selectedPageId={selectedPageId}
            inspectedFrameDump={inspectedFrameDump}
            inspectedBTreePage={inspectedBTreePage}
            inspectedPage={inspectedPage}
            onGoToTables={() => activeTab = "tables"}
            onGoToBufferPool={() => activeTab = "buffer_pool"}
        />
    </Tabs.Content>
  </Tabs.Root>
  </div>
</div>
