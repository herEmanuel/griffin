ISO_IMAGE = disk.iso
DISK_IMAGE = griffin.img
GRIFFIN = target/target/debug/griffin

.PHONY: all
all: $(ISO_IMAGE)

.PHONY: run
run: $(ISO_IMAGE) $(DISK_IMAGE)
	qemu-system-x86_64.exe -M q35 -m 2G -boot d\
		-drive id=disk,format=raw,file=$(DISK_IMAGE),if=none \
		-device ahci,id=ahci \
		-device ide-hd,drive=disk,bus=ahci.0 \
		-serial file:CON -monitor stdio -cdrom $(ISO_IMAGE)

.PHONY: test
test: $(ISO_IMAGE)
	qemu-system-x86_64.exe -M q35 -m 2G -d int -M smm=off \
		-drive id=disk,file=griffin.img,if=none \
		-device ahci,id=ahci \
		-device ide-hd,drive=disk,bus=ahci.0 \
		-serial file:CON -monitor stdio -cdrom $(ISO_IMAGE)

.PHONY: kvm
kvm:
	qemu-system-x86_64.exe -M q35 -m 2G -serial file:CON -cdrom $(ISO_IMAGE) -accel whpx

limine:
	git clone https://github.com/limine-bootloader/limine.git --branch=v2.0-branch-binary --depth=1
	make -C limine

.PHONY: kernel
griffin:
	cargo build

$(ISO_IMAGE): limine griffin
	rm -rf iso_root
	mkdir -p iso_root
	cp $(GRIFFIN) \
		limine.cfg limine/limine.sys limine/limine-cd.bin limine/limine-eltorito-efi.bin iso_root/
	xorriso -as mkisofs -b limine-cd.bin \
		-no-emul-boot -boot-load-size 4 -boot-info-table \
		--efi-boot limine-eltorito-efi.bin \
		-efi-boot-part --efi-boot-image --protective-msdos-label \
		iso_root -o $(ISO_IMAGE)
	limine/limine-install $(ISO_IMAGE)
	rm -rf iso_root

$(DISK_IMAGE): 
	dd if=/dev/zero bs=1MB count=10 of=$(DISK_IMAGE)
	parted -s $(DISK_IMAGE) mklabel gpt
	parted -s $(DISK_IMAGE) mkpart primary 0% 100%
	sudo losetup -P /dev/loop0 griffin.img
	sudo mkfs.ext2 /dev/loop0p1
	sudo losetup -d /dev/loop0 

.PHONY: clean
clean:
	rm -f $(ISO_IMAGE)
