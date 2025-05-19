# copernicus-rust

Small cli tool to list and download Copernicus Program earth observation data.

It's pretty easy to use `curl` to get the data you're looking for from their STAC
API [docs](https://documentation.dataspace.copernicus.eu/APIs/STAC.html), but
this tool aims to make authentication and querying for imagery a tiny bit easier.

This tool is also a work in progress and came out of my wanting to look at earth
observation data following some of the major events that've happened over the last
few months. It's not intended to be a complete robust solution for a variety of
ends, but is an interesting basis from which to develop those things.

## Setup

Add a .env file that looks like the included template with your Copernicus Data Space Ecosystem.

Registration is [here](https://documentation.dataspace.copernicus.eu/Registration.html).


## Running / Testing

Run `cargo run -- --bbox=-75.201704,39.981552,-75.114099,39.915099` while in the
project folder to test, after having added an .env file. Authentication and list
operations may take some time depending on other query parameters provided. You
can review what these are with `--help`.

Downloading products, e.g. archives of imagery, is also something you can do, and
works based on IDs passed to that subcommand.

## Contributing

Contributions are welcome if you like the tool and want to add something. I'm
also happy to entertain feature requests if there's something you'd like to see
but aren't sure how to implement it. For anything else, questions are welcome!
