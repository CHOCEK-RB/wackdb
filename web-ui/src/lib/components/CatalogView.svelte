<script lang="ts">
  import * as Card from "$lib/components/ui/card";
  import { Button } from "$lib/components/ui/button";
  import type { TableMetadataDto } from "$lib/types";

  let { 
      catalogTables, 
      selectedTable, 
      tableHeapNumPages, 
      tableIndexNumPages, 
      onSelectTable, 
      onViewHeapPage, 
      onViewBTreePage 
  } = $props<{
      catalogTables: TableMetadataDto[],
      selectedTable: TableMetadataDto | null,
      tableHeapNumPages: number,
      tableIndexNumPages: number,
      onSelectTable: (table: TableMetadataDto) => void,
      onViewHeapPage: (fileId: number, pageNum: number) => void,
      onViewBTreePage: (fileId: number, pageNum: number) => void
  }>();
</script>

<div class="grid grid-cols-1 md:grid-cols-4 gap-8">
    <div class="md:col-span-1">
        <Card.Root>
            <Card.Header>
                <Card.Title>Catalog</Card.Title>
                <Card.Description>Registered tables</Card.Description>
            </Card.Header>
            <Card.Content>
                {#if catalogTables.length === 0}
                    <p class="text-muted-foreground text-sm">No tables found.</p>
                {:else}
                    <div class="flex flex-col gap-2">
                        {#each catalogTables as table}
                            <Button 
                                variant={selectedTable?.name === table.name ? "default" : "outline"}
                                class="justify-start w-full"
                                onclick={() => onSelectTable(table)}
                            >
                                {table.name}
                            </Button>
                        {/each}
                    </div>
                {/if}
            </Card.Content>
        </Card.Root>
    </div>
    <div class="md:col-span-3">
        {#if selectedTable}
            <Card.Root>
                <Card.Header>
                    <Card.Title>Table: {selectedTable.name}</Card.Title>
                    <Card.Description>
                        Heap File ID: {selectedTable.heap_relation_id} | 
                        Index File ID: {selectedTable.index_relation_id}
                    </Card.Description>
                </Card.Header>
                <Card.Content>
                    <h3 class="text-sm font-bold text-muted-foreground mb-4 border-b pb-2">HEAP PAGES (SLOTTED PAGES)</h3>
                    {#if tableHeapNumPages === 0}
                        <p class="text-muted-foreground mb-8">This table has no heap pages yet.</p>
                    {:else}
                        <div class="grid grid-cols-4 sm:grid-cols-6 md:grid-cols-8 lg:grid-cols-12 gap-2 mb-8">
                            {#each Array(tableHeapNumPages) as _, pageNum}
                                <Button 
                                    variant="outline" 
                                    class="aspect-square flex flex-col items-center justify-center p-2 h-auto border-blue-200"
                                    onclick={() => onViewHeapPage(selectedTable!.heap_relation_id, pageNum)}
                                >
                                    <span class="text-xs text-muted-foreground">Page</span>
                                    <span class="font-bold">{pageNum}</span>
                                </Button>
                            {/each}
                        </div>
                    {/if}
                    
                    <h3 class="text-sm font-bold text-muted-foreground mb-4 border-b pb-2">INDEX PAGES (B+ TREE NODES)</h3>
                    {#if tableIndexNumPages === 0}
                        <p class="text-muted-foreground">This table has no index pages initialized yet.</p>
                    {:else}
                        <div class="grid grid-cols-4 sm:grid-cols-6 md:grid-cols-8 lg:grid-cols-12 gap-2">
                            {#each Array(tableIndexNumPages) as _, pageNum}
                                <Button 
                                    variant="outline" 
                                    class="aspect-square flex flex-col items-center justify-center p-2 h-auto border-green-200"
                                    onclick={() => onViewBTreePage(selectedTable!.index_relation_id, pageNum)}
                                >
                                    <span class="text-xs text-muted-foreground">Node</span>
                                    <span class="font-bold">{pageNum}</span>
                                </Button>
                            {/each}
                        </div>
                    {/if}
                </Card.Content>
            </Card.Root>
        {:else}
            <Card.Root class="h-full flex items-center justify-center min-h-[300px]">
                <Card.Content class="text-center text-muted-foreground">
                    Select a table from the catalog to view its physical pages.
                </Card.Content>
            </Card.Root>
        {/if}
    </div>
</div>
