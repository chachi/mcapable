package main

import (
	"fmt"
	"log"

	"github.com/mcapable/mcapable-go"
	"mcapable-go-examples/common"
)

func main() {
	reader, err := mcapable.NewReaderFromBytes(common.SampleBytes())
	if err != nil {
		log.Fatal(err)
	}

	stream, err := reader.Chunks()
	if err != nil {
		log.Fatal(err)
	}
	count := 0
	for {
		chunk, err := stream.Next()
		if err != nil {
			log.Fatal(err)
		}
		if chunk == nil {
			break
		}
		count++
		fmt.Printf("Chunk: start=%d end=%d compression=%s size=%d records=%d\n",
			chunk.MessageStartTime,
			chunk.MessageEndTime,
			chunk.Compression,
			chunk.UncompressedSize,
			len(chunk.Records),
		)
	}
	fmt.Printf("Total chunks: %d\n", count)
	stream.Close()
}
