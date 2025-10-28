use std::io::Cursor;
use std::slice;

use anyhow::Result;
use arrow::array::{Array, Float32Array, ListArray, StringArray, UInt32Array, UInt64Array};
use arrow::ipc::reader::StreamReader;
use zenrc_shm::shm::MemoryHandle;

fn main() -> Result<()> {
    // 共享内存名称
    let name = "/my_shared_mem_arrow";
    let size: usize = 4096 * 64;

    // 打开现有共享内存
    let mut mem_handle = MemoryHandle::open(name).expect("MemoryHandle::open failed");

    // 获取数据指针
    let data = unsafe { slice::from_raw_parts(mem_handle.get_mut_ptr().as_ptr(), size) };
    loop {
        // 使用 Arrow IPC Reader 解析
        let mut reader = StreamReader::try_new(Cursor::new(data), None)?;

        println!("✅ 成功读取共享内存中的 Arrow 流");

        while let Some(batch_result) = reader.next() {
            let batch = batch_result?;
            println!(
                "RecordBatch 中包含 {} 行, {} 列",
                batch.num_rows(),
                batch.num_columns()
            );

            // 提取字段
            let seq_arr = batch
                .column_by_name("seq")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt32Array>()
                .unwrap();
            let seq = seq_arr.value(0);

            let stamp_secs_arr = batch
                .column_by_name("stamp_secs")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt64Array>()
                .unwrap();
            let stamp_secs = stamp_secs_arr.value(0);

            let stamp_nsecs_arr = batch
                .column_by_name("stamp_nsecs")
                .unwrap()
                .as_any()
                .downcast_ref::<UInt32Array>()
                .unwrap();
            let stamp_nsecs = stamp_nsecs_arr.value(0);

            let frame_id_arr = batch
                .column_by_name("frame_id")
                .unwrap()
                .as_any()
                .downcast_ref::<StringArray>()
                .unwrap();
            let frame_id = frame_id_arr.value(0);

            // 提取 ranges
            let ranges_arr = batch
                .column_by_name("ranges")
                .unwrap()
                .as_any()
                .downcast_ref::<ListArray>()
                .unwrap();

            let float_values = ranges_arr.value(0);
            let float_arr = float_values
                .as_any()
                .downcast_ref::<Float32Array>()
                .unwrap();

            let mut ranges = Vec::new();
            for i in 0..float_arr.len() {
                if float_arr.is_null(i) {
                    ranges.push(f32::NAN);
                } else {
                    ranges.push(float_arr.value(i));
                }
            }

            // 打印结果
            println!("LaserScan 数据：");
            println!("  seq: {}", seq);
            println!("  stamp: {}.{}", stamp_secs, stamp_nsecs);
            println!("  frame_id: {}", frame_id);
            println!("  ranges[0..10]: {:?}", &ranges[0..ranges.len()]);
            println!("  ranges.len(): {}", ranges.len());
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    }
    Ok(())
}
