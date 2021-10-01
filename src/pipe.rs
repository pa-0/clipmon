use crate::ControlOfferUserData;
use calloop::LoopHandle;
use calloop::LoopSignal;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;

pub fn read_offer(data_offer: &ZwlrDataControlOfferV1, handle: &LoopHandle<LoopSignal>) {
    // let user_data = match data_offer
    //     .as_ref()
    //     .user_data()
    //     .get::<ControlOfferUserData>()
    // {
    //     Some(data) => data,
    //     None => {
    //         println!("We're missing data for this offer! o.O");
    //         return;
    //     }
    // };
    let user_data = data_offer
        .as_ref()
        .user_data()
        .get::<ControlOfferUserData>()
        .unwrap();

    for mime_type in user_data.mime_types.borrow().iter() {
        println!("receiving type:{}", mime_type);
        let (r, w) = os_pipe::pipe().unwrap();
        data_offer.receive(mime_type.to_string(), w.as_raw_fd());
        drop(w);

        let source = calloop::generic::Generic::new(
            r,
            calloop::Interest {
                readable: true,
                writable: false,
            },
            calloop::Mode::Edge,
        );

        let mime_type = mime_type.clone();
        let id = data_offer.as_ref().id().clone();
        handle
            .insert_source(source, move |_event, reader, _data| {
                let mut reader = std::io::BufReader::new(reader);
                let mut buf = Vec::<u8>::new();
                let len = reader.read_to_end(&mut buf)?;

                println!(
                    "read. data_offer: {:?}/{}: {:?} {:?}, {:?}",
                    id, mime_type, len, buf, reader
                );

                // Given that we've read all the data, no need to continue
                // having this source in the event loop:
                return Result::Ok(calloop::PostAction::Remove);
                // return Result::Ok(calloop::PostAction::Continue);
            })
            .unwrap();
    }
}
