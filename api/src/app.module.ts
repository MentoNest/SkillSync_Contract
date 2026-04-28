import { Module } from '@nestjs/common';
import { ContractsModule } from './contracts/contracts.module';

@Module({
  imports: [ContractsModule]
})
export class AppModule {}
