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
	defer reader.Close()

	schemas, err := reader.Schemas()
	if err != nil {
		log.Fatal(err)
	}
	channels, err := reader.Channels()
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("Schemas: %d\n", len(schemas))
	fmt.Printf("Channels: %d\n", len(channels))

	fmt.Println("\nChannels:")
	for _, channel := range channels {
		fmt.Printf("  [%d] topic='%s' encoding='%s' schema_id=%d\n",
			channel.ID,
			channel.Topic,
			channel.MessageEncoding,
			channel.SchemaID,
		)
	}

	fmt.Println("\nSchemas:")
	for _, schema := range schemas {
		fmt.Printf("  [%d] name='%s' encoding='%s' data_len=%d\n",
			schema.ID,
			schema.Name,
			schema.Encoding,
			len(schema.Data),
		)
	}
}
