ISO_IMAGE = disk.iso
GRIFFIN = target/target/debug/griffin

.PHONY: all
all: $(ISO_IMAGE)

.PHONY: run
run: $(ISO_IMAGE)
	qemu-system-x86_64.exe -M q35 -m 2G -serial file:CON -cdrom $(ISO_IMAGE)

.PHONY: test
test: $(ISO_IMAGE)
	qemu-system-x86_64.exe -M q35 -m 2G -d int -M smm=off -serial file:CON -cdrom $(ISO_IMAGE)

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

.PHONY: clean
clean:
	rm -f $(ISO_IMAGE)
