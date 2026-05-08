mod pet_window;

use pet_window::PetWindow;

fn main() {
    PetWindow::run().expect("宠物窗口启动失败");
}
