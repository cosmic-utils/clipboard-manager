
extern crate serde;

use serde::{Deserialize, Serialize};







#[derive(Clone, Debug, Serialize, Deserialize)]
struct Payload {
    
    mime: String,
    value: String,
}


#[test]
pub fn clear() {
    let tree = sled::open("/tmp/welcome-to-sled").expect("open");
    tree.clear().unwrap();
}

#[test]
pub fn init() {

    let tree = sled::open("/tmp/welcome-to-sled").expect("open");
    
    use std::time::{SystemTime, UNIX_EPOCH};

    let s = Payload {
        mime: "text".into(),
        value: "data3".into()
    };

    let start = SystemTime::now();
    let since_the_epoch = start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards");

    let mut f = Vec::new();

    bincode::serialize_into(&mut f, &s).unwrap();

    let key = U128(since_the_epoch.as_millis());

    tree.insert(&key, f).unwrap();

    tree.flush().unwrap();

    let res = tree.get(&key).unwrap().unwrap();

    let o = bincode::deserialize::<Payload>(res.as_ref()).unwrap();

    dbg!(&o);
}

#[test]
pub fn read() {
    let tree = sled::open("/tmp/welcome-to-sled").expect("open");

    tree.iter().rev().for_each(|i| {
        
        let Ok((key, value)) = i else {
            panic!("iter");
        };

        let key = bincode::deserialize::<U128>(&key).unwrap();

        let value = bincode::deserialize::<Payload>(&value).unwrap();

        dbg!(&key, &value);
      
    });

  
}


#[derive(Clone, Debug, Serialize, Deserialize)]
struct U128(u128);


impl AsRef<[u8]> for U128 {
    fn as_ref(&self) -> &[u8] {
        let size = std::mem::size_of::<u128>();
        // We can use `std::slice::from_raw_parts` to create a slice from the u128 value
        // This is done by casting the reference to a pointer and then creating a slice from it
        unsafe { std::slice::from_raw_parts(self as *const Self as *const u8, size) }
    
    }
}