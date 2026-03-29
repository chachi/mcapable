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

	msgStream, err := reader.Messages()
	if err != nil {
		log.Fatal(err)
	}
	first, err := msgStream.Next()
	if err != nil {
		log.Fatal(err)
	}
	if first != nil {
		fmt.Printf("First message: channel=%d time=%d\n", first.ChannelID, first.LogTime)
	}

	reader, err = msgStream.IntoReader()
	if err != nil {
		log.Fatal(err)
	}

	rawStream, err := reader.RawMessages()
	if err != nil {
		log.Fatal(err)
	}
	count := 0
	for {
		msg, err := rawStream.Next()
		if err != nil {
			log.Fatal(err)
		}
		if msg == nil {
			break
		}
		count++
	}
	fmt.Printf("Raw message count: %d\n", count)
	rawStream.Close()
}
