<script lang="ts">
  import * as Card from "$lib/components/ui/card";
  import * as Table from "$lib/components/ui/table";
  import { Button } from "$lib/components/ui/button";
  import type { FrameDumpDto, PageDto, BTreePageDto } from "$lib/types";

  let { 
      inspectorMode, 
      selectedFrameId, 
      selectedPageId, 
      inspectedFrameDump, 
      inspectedBTreePage, 
      inspectedPage,
      onGoToTables,
      onGoToBufferPool
  } = $props<{
      inspectorMode: "frame" | "heap_page" | "btree_page" | null,
      selectedFrameId: number | null,
      selectedPageId: { file: number, page: number } | null,
      inspectedFrameDump: FrameDumpDto | null,
      inspectedBTreePage: BTreePageDto | null,
      inspectedPage: PageDto | null,
      onGoToTables: () => void,
      onGoToBufferPool: () => void
  }>();

  function hexToAscii(hexStr: string): string {
      let ascii = "";
      for (let i = 0; i < hexStr.length; i += 2) {
          const charCode = parseInt(hexStr.substring(i, i + 2), 16);
          // Printable ASCII is 32 to 126
          if (charCode >= 32 && charCode <= 126) {
              ascii += String.fromCharCode(charCode);
          } else {
              ascii += ".";
          }
      }
      return ascii;
  }
</script>

{#if inspectorMode !== null}
<Card.Root class="border-primary">
  <Card.Header>
    <Card.Title>
        {#if inspectorMode === "btree_page"}B-Tree Node Inspector{:else}Slotted Page Inspector{/if} 
        {#if inspectorMode === "frame"}
            (Frame {selectedFrameId})
        {:else}
            (File {selectedPageId?.file}, Page {selectedPageId?.page})
        {/if}
    </Card.Title>
    <Card.Description>
        {#if inspectorMode === "btree_page"}
            Viewing B+ Tree physical node.
        {:else}
            Viewing Heap physical slotted page contents.
        {/if}
    </Card.Description>
  </Card.Header>
  <Card.Content>
    {#if inspectorMode === "frame"}
        <!-- Raw Frame Hex Dump -->
        {#if inspectedFrameDump}
            <div class="mb-4">
                <h3 class="text-lg font-semibold mb-2">Raw Frame Content (Hex Dump)</h3>
                <p class="text-xs text-muted-foreground mb-2">This is the exact sequence of 8192 bytes currently residing in RAM for this frame. No parsing is applied.</p>
                <div class="bg-muted p-4 rounded-md font-mono text-[10px] break-all leading-tight max-h-[500px] overflow-y-auto">
                    {inspectedFrameDump.hex_dump}
                </div>
            </div>
        {:else}
            <p class="text-muted-foreground italic">Frame is empty or data is unavailable.</p>
        {/if}
    {:else if inspectorMode === "btree_page"}
        {#if inspectedBTreePage}
            <div class="grid grid-cols-1 md:grid-cols-2 gap-8">
                <!-- Header Info -->
                <div>
                    <h3 class="text-lg font-semibold mb-2">B+ Tree Node Header</h3>
                    <div class="bg-muted p-4 rounded-md flex flex-col gap-2 font-mono text-sm border-l-4 border-green-500">
                        <div class="flex justify-between"><span>Node Type:</span> <span class="font-bold text-green-600">{inspectedBTreePage.header.node_type}</span></div>
                        <div class="flex justify-between"><span>Keys:</span> <span>{inspectedBTreePage.header.num_keys} / {inspectedBTreePage.header.max_keys}</span></div>
                        <div class="flex justify-between"><span>Parent Page:</span> <span>{inspectedBTreePage.header.parent_page_num ?? "None (Root)"}</span></div>
                        <div class="flex justify-between"><span>Next Leaf:</span> <span>{inspectedBTreePage.header.next_page_num ?? "None"}</span></div>
                    </div>
                </div>
                
                <!-- Keys / Values Info -->
                <div>
                    <h3 class="text-lg font-semibold mb-2">
                        {#if inspectedBTreePage.node_data.type === "Leaf"}Keys & Values (CTIDs){:else}Keys & Children (Page IDs){/if}
                    </h3>
                    <div class="bg-muted p-4 rounded-md flex flex-col gap-2 font-mono text-sm max-h-[400px] overflow-y-auto">
                        {#if inspectedBTreePage.header.num_keys === 0}
                            <p class="text-muted-foreground">Node is empty.</p>
                        {:else}
                            <Table.Root>
                              <Table.Header>
                                <Table.Row>
                                  <Table.Head class="w-[50px]">Idx</Table.Head>
                                  <Table.Head>Key</Table.Head>
                                  <Table.Head class="text-right">
                                      {#if inspectedBTreePage.node_data.type === "Leaf"}Value (CTID){:else}Child Node{/if}
                                  </Table.Head>
                                </Table.Row>
                              </Table.Header>
                              <Table.Body>
                                {#if inspectedBTreePage.node_data.type === "Leaf"}
                                    {#each inspectedBTreePage.node_data.data.keys as key, i}
                                        <Table.Row>
                                            <Table.Cell class="font-medium text-muted-foreground">{i}</Table.Cell>
                                            <Table.Cell class="font-bold">{key}</Table.Cell>
                                            <Table.Cell class="text-right text-green-600">{inspectedBTreePage.node_data.data.values[i]}</Table.Cell>
                                        </Table.Row>
                                    {/each}
                                {:else}
                                    {#each inspectedBTreePage.node_data.data.children as child, i}
                                        <Table.Row>
                                            <Table.Cell class="font-medium text-muted-foreground">{i}</Table.Cell>
                                            <Table.Cell class="font-bold">{i < inspectedBTreePage.header.num_keys ? inspectedBTreePage.node_data.data.keys[i] : "Infinity"}</Table.Cell>
                                            <Table.Cell class="text-right text-blue-600">Page {child}</Table.Cell>
                                        </Table.Row>
                                    {/each}
                                {/if}
                              </Table.Body>
                            </Table.Root>
                        {/if}
                    </div>
                </div>
            </div>
        {:else}
            <p class="text-muted-foreground italic">Node is empty or data is unavailable.</p>
        {/if}
    {:else}
        <!-- Slotted Page Inspector -->
        {#if inspectedPage}
          <div class="grid grid-cols-1 md:grid-cols-2 gap-8">
              <!-- Header Info -->
              <div>
                  <h3 class="text-lg font-semibold mb-2">Page Header</h3>
                  <div class="bg-muted p-4 rounded-md flex flex-col gap-2 font-mono text-sm border-l-4 border-blue-500">
                      <div class="flex justify-between"><span>LSN:</span> <span>{inspectedPage.header.lsn}</span></div>
                      <div class="flex justify-between"><span>Total Slots:</span> <span>{inspectedPage.header.total_slots}</span></div>
                      <div class="flex justify-between"><span>Free Lower:</span> <span>{inspectedPage.header.free_space_lower}</span></div>
                      <div class="flex justify-between"><span>Free Upper:</span> <span>{inspectedPage.header.free_space_upper}</span></div>
                      <div class="flex justify-between"><span>Flags:</span> <span>{inspectedPage.header.page_flags}</span></div>
                  </div>
              </div>
              
              <!-- Slots & Records Info -->
              <div>
                  <h3 class="text-lg font-semibold mb-2">Tuples (Records)</h3>
                  <div class="bg-muted p-4 rounded-md flex flex-col gap-2 font-mono text-sm max-h-[400px] overflow-y-auto">
                      {#if inspectedPage.records.length === 0}
                          <p class="text-muted-foreground">No records found on this page.</p>
                      {:else}
                          {#each inspectedPage.records as record}
                              <div class="border-b border-border pb-2 mb-2 last:border-0 last:pb-0 last:mb-0">
                                  <div class="text-primary font-bold">Slot {record.slot_idx}</div>
                                  <div class="flex justify-between text-xs"><span>xmin:</span> <span>{record.xmin}</span></div>
                                  <div class="flex justify-between text-xs"><span>xmax:</span> <span>{record.xmax}</span></div>
                                  <div class="mt-2 bg-background p-2 rounded border border-border">
                                      <div class="text-[10px] text-muted-foreground mb-1 font-bold">HEXADECIMAL</div>
                                      <div class="break-all text-xs mb-2">0x{record.data_hex}</div>
                                      <div class="text-[10px] text-muted-foreground mb-1 font-bold">TEXT (ASCII / xxd)</div>
                                      <div class="break-all text-xs text-green-500 whitespace-pre-wrap">{hexToAscii(record.data_hex)}</div>
                                  </div>
                              </div>
                          {/each}
                      {/if}
                  </div>
              </div>
          </div>
        {:else}
          <p class="text-muted-foreground italic">Frame is empty or data is unavailable.</p>
        {/if}
    {/if}
  </Card.Content>
</Card.Root>
{:else}
<Card.Root>
  <Card.Header>
    <Card.Title>Page Inspector</Card.Title>
    <Card.Description>Select a frame from the Buffer Pool or a page from a Table to inspect its contents.</Card.Description>
  </Card.Header>
  <Card.Content class="flex gap-4">
     <Button variant="outline" onclick={onGoToTables}>Go to Tables</Button>
     <Button variant="outline" onclick={onGoToBufferPool}>Go to Buffer Pool</Button>
  </Card.Content>
</Card.Root>
{/if}
