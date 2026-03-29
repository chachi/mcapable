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
	header, err := reader.Header()
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("Profile: %s\n", header.Profile)
	fmt.Printf("Library: %s\n", header.Library)

	stream, err := reader.RawMessages()
	if err != nil {
		log.Fatal(err)
	}
	count := 0
	for {
		msg, err := stream.Next()
		if err != nil {
			log.Fatal(err)
		}
		if msg == nil {
			break
		}
		count++
		if count <= 3 {
			fmt.Printf("Message: channel=%d time=%d size=%d\n", msg.ChannelID, msg.LogTime, len(msg.Data))
		}
	}
	fmt.Printf("Total messages: %d\n", count)
	stream.Close()
}
