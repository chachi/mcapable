package main

import (
	"encoding/json"
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

	stream, err := reader.Messages()
	if err != nil {
		log.Fatal(err)
	}
	parsed, err := stream.Parsed()
	if err != nil {
		log.Fatal(err)
	}

	count := 0
	for {
		msg, err := parsed.Next()
		if err != nil {
			log.Fatal(err)
		}
		if msg == nil {
			break
		}
		count++
		if msg.IsJSON {
			data, _ := json.Marshal(msg.JSON)
			fmt.Printf("Parsed JSON: %s\n", string(data))
		} else {
			fmt.Printf("Parsed bytes: %d\n", len(msg.Bytes))
		}
		if count >= 3 {
			break
		}
	}
	parsed.Close()
}
