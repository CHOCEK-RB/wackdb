<script lang="ts">
  import * as Card from "$lib/components/ui/card";
  import { Badge } from "$lib/components/ui/badge";
  import * as Table from "$lib/components/ui/table";
  import { Button } from "$lib/components/ui/button";
  import type { BufferPoolStateDto } from "$lib/types";

  let { bufferData, onViewFrame } = $props<{
      bufferData: BufferPoolStateDto,
      onViewFrame: (frameId: number) => void
  }>();
</script>

<div class="flex flex-col gap-4">
  <div class="grid grid-cols-1 md:grid-cols-3 gap-4">
    <Card.Root>
      <Card.Header>
        <Card.Title>Cache Hits</Card.Title>
        <Card.Description>Total buffer hits</Card.Description>
      </Card.Header>
      <Card.Content>
        <p class="text-4xl font-bold text-green-500">{bufferData.hits}</p>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title>Cache Misses</Card.Title>
        <Card.Description>Total buffer misses</Card.Description>
      </Card.Header>
      <Card.Content>
        <p class="text-4xl font-bold text-red-500">{bufferData.misses}</p>
      </Card.Content>
    </Card.Root>

    <Card.Root>
      <Card.Header>
        <Card.Title>Hit Rate</Card.Title>
        <Card.Description>Cache efficiency</Card.Description>
      </Card.Header>
      <Card.Content>
        <p class="text-4xl font-bold text-blue-500">{(bufferData.hit_rate * 100).toFixed(2)}%</p>
      </Card.Content>
    </Card.Root>
  </div>

  <Card.Root>
    <Card.Header>
      <Card.Title>Memory Frames</Card.Title>
      <Card.Description>Physical memory pages managed by the Buffer Pool.</Card.Description>
    </Card.Header>
    <Card.Content>
      <div class="border rounded-md">
        <Table.Root>
          <Table.Header>
            <Table.Row>
              <Table.Head class="w-[100px]">Frame ID</Table.Head>
              <Table.Head>Page ID (File:Num)</Table.Head>
              <Table.Head>Pin Count</Table.Head>
              <Table.Head>Status</Table.Head>
              <Table.Head class="text-right">Actions</Table.Head>
            </Table.Row>
          </Table.Header>
          <Table.Body>
            {#each bufferData.frames as frame (frame.frame_id)}
              <Table.Row>
                <Table.Cell class="font-medium">{frame.frame_id}</Table.Cell>
                <Table.Cell>
                  {#if frame.page_id}
                    {frame.page_id.file_id} : {frame.page_id.page_num}
                  {:else}
                    <span class="text-muted-foreground italic">Empty</span>
                  {/if}
                </Table.Cell>
                <Table.Cell>
                  <Badge variant={frame.pin_count > 0 ? "default" : "secondary"}>
                    {frame.pin_count}
                  </Badge>
                </Table.Cell>
                <Table.Cell>
                  {#if frame.is_dirty}
                    <Badge variant="destructive">Dirty</Badge>
                  {:else}
                    <Badge variant="outline" class="text-green-600 border-green-600">Clean</Badge>
                  {/if}
                </Table.Cell>
                <Table.Cell class="text-right">
                  <Button variant="ghost" size="sm" onclick={() => onViewFrame(frame.frame_id)}>
                    Inspect
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
