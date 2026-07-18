<script lang="ts">
  import * as Card from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import * as Table from "$lib/components/ui/table";
  import { Button } from "$lib/components/ui/button";
  import { Target, HardDriveDownload, Percent, Zap } from "lucide-svelte";
  import type { BufferPoolStateDto } from "$lib/types";

  let { bufferData, onViewFrame } = $props<{
      bufferData: BufferPoolStateDto,
      onViewFrame: (frameId: number) => void
  }>();

  let hitRatePercent = $derived((bufferData.hit_rate * 100).toFixed(2));
</script>

<div class="flex flex-col gap-8 animate-in fade-in slide-in-from-bottom-4 duration-500">
  <!-- Top Metrics Cards -->
  <div class="grid grid-cols-1 md:grid-cols-3 gap-6">
    <Card.Root class="relative overflow-hidden group hover:border-green-500/50 transition-colors">
      <div class="absolute inset-0 bg-gradient-to-br from-green-500/10 to-transparent opacity-0 group-hover:opacity-100 transition-opacity"></div>
      <Card.Header class="flex flex-row items-center justify-between pb-2">
        <div class="flex flex-col gap-1">
          <Card.Title class="text-lg font-medium text-muted-foreground">Cache Hits</Card.Title>
          <Card.Description>Served from RAM</Card.Description>
        </div>
        <div class="p-3 bg-green-500/10 rounded-xl ring-1 ring-green-500/20 text-green-500">
          <Target class="w-5 h-5" />
        </div>
      </Card.Header>
      <Card.Content>
        <p class="text-5xl font-black text-green-500 tracking-tighter drop-shadow-sm">{bufferData.hits}</p>
      </Card.Content>
    </Card.Root>

    <Card.Root class="relative overflow-hidden group hover:border-red-500/50 transition-colors">
      <div class="absolute inset-0 bg-gradient-to-br from-red-500/10 to-transparent opacity-0 group-hover:opacity-100 transition-opacity"></div>
      <Card.Header class="flex flex-row items-center justify-between pb-2">
        <div class="flex flex-col gap-1">
          <Card.Title class="text-lg font-medium text-muted-foreground">Cache Misses</Card.Title>
          <Card.Description>Disk I/O required</Card.Description>
        </div>
        <div class="p-3 bg-red-500/10 rounded-xl ring-1 ring-red-500/20 text-red-500">
          <HardDriveDownload class="w-5 h-5" />
        </div>
      </Card.Header>
      <Card.Content>
        <p class="text-5xl font-black text-red-500 tracking-tighter drop-shadow-sm">{bufferData.misses}</p>
      </Card.Content>
    </Card.Root>

    <Card.Root class="relative overflow-hidden group hover:border-blue-500/50 transition-colors">
      <div class="absolute inset-0 bg-gradient-to-br from-blue-500/10 to-transparent opacity-0 group-hover:opacity-100 transition-opacity"></div>
      <Card.Header class="flex flex-row items-center justify-between pb-2">
        <div class="flex flex-col gap-1">
          <Card.Title class="text-lg font-medium text-muted-foreground">Hit Rate</Card.Title>
          <Card.Description>Overall efficiency</Card.Description>
        </div>
        <div class="p-3 bg-blue-500/10 rounded-xl ring-1 ring-blue-500/20 text-blue-500">
          <Percent class="w-5 h-5" />
        </div>
      </Card.Header>
      <Card.Content>
        <div class="flex items-baseline gap-2">
          <p class="text-5xl font-black text-blue-500 tracking-tighter drop-shadow-sm">{hitRatePercent}%</p>
        </div>
        <!-- Progress Bar -->
        <div class="mt-4 h-2 w-full bg-secondary/50 rounded-full overflow-hidden">
          <div class="h-full bg-blue-500 transition-all duration-500 ease-out" style="width: {hitRatePercent}%"></div>
        </div>
      </Card.Content>
    </Card.Root>
  </div>

  <!-- Frames Table -->
  <Card.Root class="border-border/50 shadow-sm backdrop-blur-sm bg-card/80">
    <Card.Header>
      <div class="flex items-center gap-3">
        <div class="p-2 bg-primary/10 rounded-lg">
          <Zap class="w-5 h-5 text-primary" />
        </div>
        <div>
          <Card.Title class="text-xl">Memory Frames Layout</Card.Title>
          <Card.Description>Real-time physical memory pages managed by the LRU pool.</Card.Description>
        </div>
      </div>
    </Card.Header>
    <Card.Content>
      <div class="rounded-xl border border-border/50 overflow-hidden bg-background/50">
        <Table.Root>
          <Table.Header class="bg-secondary/30">
            <Table.Row class="hover:bg-transparent">
              <Table.Head class="w-[120px] font-semibold text-primary">Frame ID</Table.Head>
              <Table.Head class="font-semibold text-primary">Mapped Page (File : Num)</Table.Head>
              <Table.Head class="font-semibold text-primary">Pin Count</Table.Head>
              <Table.Head class="font-semibold text-primary">State</Table.Head>
              <Table.Head class="text-right font-semibold text-primary">Action</Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each bufferData.frames as frame (frame.frame_id)}
              <Table.Row class="group hover:bg-secondary/20 transition-colors">
                <Table.Cell class="font-bold text-muted-foreground group-hover:text-foreground transition-colors">
                  #{frame.frame_id}
                </Table.Cell>
                <Table.Cell>
                  {#if frame.page_id}
                    <div class="flex items-center gap-2">
                      <Badge variant="outline" class="font-mono bg-background shadow-sm border-primary/20">
                        {frame.page_id.file_id}
                      </Badge>
                      <span class="text-muted-foreground">:</span>
                      <Badge variant="outline" class="font-mono bg-background shadow-sm">
                        {frame.page_id.page_num}
                      </Badge>
                    </div>
                  {:else}
                    <span class="text-muted-foreground italic bg-secondary/50 px-2 py-1 rounded-md text-sm">Unmapped</span>
                  {/if}
                </Table.Cell>
                <Table.Cell>
                  <div class="flex items-center gap-1">
                    {#each Array(Math.min(frame.pin_count, 3)) as _}
                      <div class="w-2 h-2 rounded-full bg-primary animate-pulse"></div>
                    {/each}
                    <span class="font-medium ml-2" class:text-muted-foreground={frame.pin_count === 0}>
                      {frame.pin_count}
                    </span>
                  </div>
                </Table.Cell>
                <Table.Cell>
                  {#if frame.is_dirty}
                    <Badge variant="destructive" class="bg-red-500/10 text-red-500 hover:bg-red-500/20 ring-1 ring-red-500/30">
                      <span class="w-1.5 h-1.5 rounded-full bg-red-500 mr-2 animate-pulse"></span>
                      Dirty
                    </Badge>
                  {:else if frame.page_id}
                    <Badge variant="outline" class="bg-green-500/10 text-green-500 border-green-500/30">
                      Clean
                    </Badge>
                  {:else}
                    <Badge variant="secondary" class="opacity-50">Empty</Badge>
                  {/if}
                </Table.Cell>
                <Table.Cell class="text-right">
                  <Button 
                    variant="ghost" 
                    size="sm" 
                    onclick={() => onViewFrame(frame.frame_id)}
                    class="opacity-0 group-hover:opacity-100 transition-opacity hover:bg-primary hover:text-primary-foreground"
                    disabled={!frame.page_id}
                  >
                    Inspect Hex
                  </Button>
                </Table.Cell>
              </Table.Row>
            {/each}
          </Table.Body>
        </Table.Root>
      </div>
    </Card.Content>
  </Card.Root>
</div>
