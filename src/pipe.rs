use crate::ControlOfferUserData;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use wayland_protocols::wlr::unstable::data_control::v1::client::zwlr_data_control_offer_v1::ZwlrDataControlOfferV1;

pub fn read_offer(data_offer: ZwlrDataControlOfferV1) {
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
        let (mut r, w) = os_pipe::pipe().unwrap();
        data_offer.receive(mime_type.to_string(), w.as_raw_fd());

        drop(w);

        let mime_type = mime_type.clone();
        std::thread::spawn(move || {
            const BUF_SIZE: usize = 1024;
            let mut buf = [0; BUF_SIZE];
            let mut read = 0;

            println!("pre-reading");
            loop {
                let len = match r.read(&mut buf) {
                    Ok(0) => {
                        println!("Done reading!");
                        break;
                    }
                    Ok(len) => {
                        read += len;
                        println!("Read {}", len);
                        if len < BUF_SIZE {
                            break;
                        }
                    }
                    Err(err) => {
                        println!("Error! {}", err);
                        break;
                    }
                };
                println!("read:{:?}", len);
            }

            println!("post-reading:{}bytes, {}", read, mime_type);
        });
    }
}
